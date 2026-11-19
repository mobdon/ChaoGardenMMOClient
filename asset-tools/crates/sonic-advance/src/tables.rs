//! Structural table readers. Unknown scalar fields are preserved, not renamed
//! as gameplay properties. These readers do not infer resource lengths.
use crate::{binary, Error, Result};

pub const UI_TABLE_OFFSET: usize = 0x680a38;
pub const UI_TABLE_COUNT: usize = 65;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiGraphics {
    pub tile_offset: usize,
    pub palette_offset: usize,
    pub unknown: [u32; 5],
}

pub fn ui_graphics(
    rom: &[u8],
    offset: usize,
    count: usize,
    max_records: usize,
) -> Result<Vec<UiGraphics>> {
    if count > max_records || count > 256 {
        return Err(Error("UI record limit exceeded".into()));
    }
    let bytes = binary::range(
        rom,
        offset,
        count
            .checked_mul(28)
            .ok_or_else(|| Error("UI table overflow".into()))?,
    )?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(count)
        .map_err(|_| Error("UI table allocation failed".into()))?;
    for (i, _) in bytes.as_chunks::<28>().0.iter().enumerate() {
        let at = offset + i * 28;
        output.push(UiGraphics {
            tile_offset: binary::rom_offset(binary::u32_at(rom, at)?, rom.len(), 1)?,
            palette_offset: binary::rom_offset(binary::u32_at(rom, at + 4)?, rom.len(), 2)?,
            unknown: [
                binary::u32_at(rom, at + 8)?,
                binary::u32_at(rom, at + 12)?,
                binary::u32_at(rom, at + 16)?,
                binary::u32_at(rom, at + 20)?,
                binary::u32_at(rom, at + 24)?,
            ],
        });
    }
    Ok(output)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tilemap<'a> {
    pub width_tiles: u16,
    pub height_tiles: u16,
    pub animation_tile_bytes: u16,
    pub animation_frames: u8,
    pub animation_delay: u8,
    pub tile_offset: usize,
    pub tiles: &'a [u8],
    pub palette_offset: Option<usize>,
    pub palette: Option<&'a [u8]>,
    pub palette_color_count: u16,
    pub palette_color_offset: u16,
    pub map_offset: usize,
    pub raw_header: &'a [u8],
}

/// Read the common 28-byte prefix. `map_offset` remains a pointer because
/// backgrounds can use u8/u16 indices, while foregrounds point at metatiles.
/// No index width, graphics mode or extra animation-frame extent is guessed.
pub fn tilemap(rom: &[u8], offset: usize, max_map_entries: usize) -> Result<Tilemap<'_>> {
    let h = binary::range(rom, offset, 28)?;
    let width_tiles = binary::u16_at(h, 0)?;
    let height_tiles = binary::u16_at(h, 2)?;
    let count = usize::from(width_tiles)
        .checked_mul(usize::from(height_tiles))
        .ok_or_else(|| Error("tilemap dimensions overflow".into()))?;
    if count == 0 || count > max_map_entries {
        return Err(Error("tilemap dimensions exceed budget".into()));
    }
    let tile_offset = binary::rom_offset(binary::u32_at(h, 8)?, rom.len(), 1)?;
    let tile_length = binary::u32_at(h, 12)? as usize;
    let tiles = binary::range(rom, tile_offset, tile_length)?;
    let palette_pointer = binary::u32_at(h, 16)?;
    let palette_offset = if palette_pointer == 0 {
        None
    } else {
        Some(binary::rom_offset(palette_pointer, rom.len(), 2)?)
    };
    let palette_color_offset = binary::u16_at(h, 20)?;
    let palette_colors = binary::u16_at(h, 22)?;
    if palette_colors == 0 || u32::from(palette_color_offset) + u32::from(palette_colors) > 256 {
        return Err(Error("invalid tilemap palette span".into()));
    }
    // Native stage backgrounds can request a palette shared by the caller.
    let palette = palette_offset
        .map(|at| binary::range(rom, at, usize::from(palette_colors) * 2))
        .transpose()?;
    Ok(Tilemap {
        width_tiles,
        height_tiles,
        animation_tile_bytes: binary::u16_at(h, 4)?,
        animation_frames: h[6],
        animation_delay: h[7],
        tile_offset,
        tiles,
        palette_offset,
        palette,
        palette_color_count: palette_colors,
        palette_color_offset,
        map_offset: binary::rom_offset(binary::u32_at(h, 24)?, rom.len(), 1)?,
        raw_header: h,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForegroundMap<'a> {
    pub tileset: Tilemap<'a>,
    pub metatile_map_offset: usize,
    pub width_metatiles: u16,
    pub height_metatiles: u16,
    pub raw_header: &'a [u8],
}

pub fn foreground_map(rom: &[u8], offset: usize, max_entries: usize) -> Result<ForegroundMap<'_>> {
    let h = binary::range(rom, offset, 36)?;
    let tileset = tilemap(rom, offset, max_entries)?;
    let width_metatiles = binary::u16_at(h, 32)?;
    let height_metatiles = binary::u16_at(h, 34)?;
    let count = usize::from(width_metatiles)
        .checked_mul(usize::from(height_metatiles))
        .ok_or_else(|| Error("foreground dimensions overflow".into()))?;
    if count == 0 || count > max_entries {
        return Err(Error("foreground dimensions exceed budget".into()));
    }
    Ok(ForegroundMap {
        tileset,
        metatile_map_offset: binary::rom_offset(binary::u32_at(h, 28)?, rom.len(), 1)?,
        width_metatiles,
        height_metatiles,
        raw_header: h,
    })
}

pub fn asoe_ui_graphics(rom: &[u8]) -> Result<Vec<UiGraphics>> {
    binary::verify_asoe(rom)?;
    ui_graphics(rom, UI_TABLE_OFFSET, UI_TABLE_COUNT, UI_TABLE_COUNT)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SongEntry {
    pub header_offset: Option<usize>,
    pub player_fields: [u16; 2],
}

/// m4a/Sappy table representation: header pointer plus two 16-bit player
/// fields. A null entry is preserved. No sequence or sample code is executed.
pub fn song_table(
    rom: &[u8],
    offset: usize,
    count: usize,
    max_records: usize,
) -> Result<Vec<SongEntry>> {
    if count > max_records || count > 4096 {
        return Err(Error("song record limit exceeded".into()));
    }
    let bytes = binary::range(
        rom,
        offset,
        count
            .checked_mul(8)
            .ok_or_else(|| Error("song table overflow".into()))?,
    )?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(count)
        .map_err(|_| Error("song table allocation failed".into()))?;
    for (i, _) in bytes.as_chunks::<8>().0.iter().enumerate() {
        let at = offset + i * 8;
        let pointer = binary::u32_at(rom, at)?;
        output.push(SongEntry {
            header_offset: if pointer == 0 {
                None
            } else {
                Some(binary::rom_offset(pointer, rom.len(), 4)?)
            },
            player_fields: [binary::u16_at(rom, at + 4)?, binary::u16_at(rom, at + 6)?],
        });
    }
    Ok(output)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SongHeader {
    pub block_count: u8,
    pub priority: u8,
    pub reverb: u8,
    pub voicegroup_offset: Option<usize>,
    pub track_offsets: Vec<usize>,
}

pub fn song_header(rom: &[u8], offset: usize, max_tracks: usize) -> Result<SongHeader> {
    let h = binary::range(rom, offset, 4)?;
    let count = usize::from(h[0]);
    if count > max_tracks || count > 16 {
        return Err(Error("song track limit exceeded".into()));
    }
    // The native zero-track dummy is four bytes, with no voicegroup pointer.
    if count == 0 {
        return Ok(SongHeader {
            block_count: h[1],
            priority: h[2],
            reverb: h[3],
            voicegroup_offset: None,
            track_offsets: Vec::new(),
        });
    }
    let extended = binary::range(rom, offset, 8)?;
    let voicegroup_offset = Some(binary::rom_offset(
        binary::u32_at(extended, 4)?,
        rom.len(),
        4,
    )?);
    let tracks_at = offset
        .checked_add(8)
        .ok_or_else(|| Error("song header overflow".into()))?;
    let track_offsets = binary::pointer_table(rom, tracks_at, count, max_tracks)?;
    Ok(SongHeader {
        block_count: h[1],
        priority: h[2],
        reverb: h[3],
        voicegroup_offset,
        track_offsets,
    })
}
