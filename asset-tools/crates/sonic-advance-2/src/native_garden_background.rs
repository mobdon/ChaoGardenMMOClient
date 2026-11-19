//! Static BG2/BG3 from the embedded Garden's InitGarden. OBJ, window and fade
//! tasks run later; this view is the unblended background, not a game frame.
use crate::{Error, VerifiedRom, bounded_range, gba, graphics};
use graphics::{RenderOptions, RenderedMap, TileDepth};

fn unzip(rom: &VerifiedRom<'_>, offset: usize, size: usize) -> Result<Vec<u8>, Error> {
    let data = rom
        .garden_bytes()
        .get(offset..)
        .ok_or_else(|| Error("Missing Garden stream".into()))?;
    let bytes = gba::decode_lz77(data, size)
        .map_err(|e| Error(e.to_string()))?
        .bytes;
    if bytes.len() != size {
        return Err(Error("Unexpected Garden stream size".into()));
    }
    Ok(bytes)
}

fn write_map(bank: &mut [u8], map: &[u8], width: usize, tile_base: u16) {
    for (i, pair) in map.as_chunks::<2>().0.iter().enumerate() {
        let p = ((i / width) * 32 + (i % width)) * 2;
        let word = u16::from_le_bytes([pair[0], pair[1]]).wrapping_add(tile_base);
        bank[p..p + 2].copy_from_slice(&word.to_le_bytes());
    }
}

fn background_vram(rom: &VerifiedRom<'_>, alternate: bool) -> Result<Vec<u8>, Error> {
    let mut vram = vec![0; 65536];
    for (src, dest, size) in [
        (0x235fc, 0x1c00, 9696),
        (0x22c44, 0x41e0, 5120),
        (0x220ec, 0x55e0, 10784),
        (0x211c8, 0x8000, 7680),
    ] {
        vram[dest..dest + size].copy_from_slice(&unzip(rom, src, size)?);
    }
    for group in 0..10 {
        let dest = 0xf80 + group * 320;
        vram.copy_within(0x2980..0x2ac0, dest);
        for i in 0..320 {
            vram[dest + i] |= vram[0x2ac0 + group * 32 + i % 32];
        }
    }
    let maps = unzip(rom, 0x24618, 720)?;
    let strip = bounded_range(&maps, usize::from(alternate) * 360, 360)?;
    write_map(&mut vram[0xf000..0xf800], strip, 9, 0xe0);
    write_map(
        &mut vram[0xf800..],
        rom.range("garden", 0x21bc4, 880)?,
        22,
        0x200,
    );
    Ok(vram)
}

/// Initial BG2 (sidebar, 4bpp) over BG3 (garden, 8bpp). The flag selects the
/// original map branch at 0x020018f4; it is not a Chao type or a save field.
pub fn render(rom: &VerifiedRom<'_>, alternate: bool) -> Result<RenderedMap, Error> {
    let vram = background_vram(rom, alternate)?;
    let palette = graphics::parse_palette(rom.range("garden", 0x233fc, 512)?, 256).map_err(gfx)?;
    let registers = rom.range("garden", 0x20e9c, 13 * 4)?;
    let reg = |id: u16| -> Result<usize, Error> {
        registers
            .as_chunks::<4>()
            .0
            .iter()
            .find(|p| u16::from_le_bytes([p[0], p[1]]) == id)
            .map(|p| usize::from(u16::from_le_bytes([p[2], p[3]])))
            .ok_or_else(|| Error(format!("Missing Garden display register {id:#x}")))
    };
    let backdrop = palette[0].rgb8();
    let mut rgba = [backdrop[0], backdrop[1], backdrop[2], 255].repeat(240 * 160);
    for (control, xreg, yreg) in [(0xe, 0x1c, 0x1e), (0xc, 0x18, 0x1a)] {
        let cnt = reg(control)?;
        let bank = (cnt >> 8 & 31) * 2048;
        let chars = (cnt >> 2 & 3) * 16384;
        let color256 = cnt & 0x80 != 0;
        let tiles = if color256 {
            graphics::decode_8bpp_tiles(&vram[chars..], 1024)
        } else {
            graphics::decode_4bpp_tiles(bounded_range(&vram, chars, 32768)?, 1024)
        }
        .map_err(gfx)?;
        let map = graphics::parse_tilemap(bounded_range(&vram, bank, 2048)?, 32, 32, 1024)
            .map_err(gfx)?;
        let layer = graphics::render_text_map(
            &tiles,
            &map,
            &palette,
            RenderOptions {
                width_tiles: 32,
                height_tiles: 32,
                depth: if color256 {
                    TileDepth::EightBpp
                } else {
                    TileDepth::FourBpp
                },
                transparent_zero: true,
                max_pixels: 256 * 256,
            },
        )
        .map_err(gfx)?;
        let (dx, dy) = (reg(xreg)?, reg(yreg)?);
        for y in 0..160 {
            for x in 0..240 {
                let src = 4 * (((dy + y) & 255) * 256 + ((dx + x) & 255));
                if layer.rgba[src + 3] != 0 {
                    let dst = 4 * (240 * y + x);
                    rgba[dst..dst + 4].copy_from_slice(&layer.rgba[src..src + 4]);
                }
            }
        }
    }
    Ok(RenderedMap {
        width: 240,
        height: 160,
        rgba,
    })
}
fn gfx(e: graphics::GraphicsError) -> Error {
    Error(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires local verified A2NE_ROM"]
    fn native_init_maps_and_pixels_match_thumb() {
        let bytes = std::fs::read(std::env::var("A2NE_ROM").unwrap()).unwrap();
        let rom = VerifiedRom::parse(&bytes).unwrap();
        for (variant, ram, pixels) in [
            (
                false,
                "f5769f7ee9bff02adf4f4b392f8ec1a8ecd7cdea5ca37fcf47c82790ecd76a62",
                "85092d6efa1c9bba6394c1733de9f7e0b8d17e3442663438c6c1735ecd22dfca",
            ),
            (
                true,
                "510f027d78522225d9e2b60e75ea3a3f6c6d2102bb8a3734ca9ce94e48d1c57e",
                "7f4af9cab17d2f44c57208630f46577a9488e599f57eda2673c0840a75d721ce",
            ),
        ] {
            assert_eq!(gba::sha256(&background_vram(&rom, variant).unwrap()), ram);
            assert_eq!(gba::sha256(&render(&rom, variant).unwrap().rgba), pixels);
        }
    }
}
