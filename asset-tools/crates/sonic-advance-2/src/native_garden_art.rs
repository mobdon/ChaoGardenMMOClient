//! Cartridge artwork inspection of playable Memory and Bounce views.
//! Data layout and OBJ descriptor decoding adapted from tiny-game-presentation.
use crate::native_garden_memory::NativeMemoryView;
use crate::native_garden_second::{BounceActorView, BounceAffineView, NativeBounceView};
use crate::{VerifiedRom, gba, graphics, graphics::RenderedMap};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn native_lz(source: &[u8], offset: usize) -> Result<gba::DecodedLz77> {
    let input = source
        .get(offset..)
        .ok_or("native stream offset outside payload")?;
    let signature = *input.first().ok_or("truncated native stream")?;
    if ![1, 0x10].contains(&signature) {
        return Err(
            format!("unmapped native compression signature {signature:#x} at {offset:#x}").into(),
        );
    }
    if signature == 0x10 {
        return Ok(gba::decode_lz77(input, 0x10000)?);
    }
    let mut normalized = input.to_vec();
    normalized[0] = 0x10;
    Ok(gba::decode_lz77(&normalized, 0x10000)?)
}
pub(crate) struct Scene<'a> {
    source: &'a [u8],
    vram: Vec<u8>,
    bg_palette: Vec<u8>,
    obj_palette: Vec<u8>,
}
impl<'a> Scene<'a> {
    pub(crate) fn new(source: &'a [u8]) -> Self {
        Self {
            source,
            vram: vec![0; 0x18000],
            bg_palette: vec![0; 512],
            obj_palette: vec![0; 512],
        }
    }
    pub(crate) fn load(
        &mut self,
        _id: &str,
        offset: usize,
        space: &str,
        destination: usize,
    ) -> Result<Vec<u8>> {
        let decoded = native_lz(self.source, offset)?;
        let target = match space {
            "vram" => &mut self.vram,
            "bgPalette" => &mut self.bg_palette,
            "objPalette" => &mut self.obj_palette,
            _ => return Err("unknown native destination".into()),
        };
        let end = destination
            .checked_add(decoded.bytes.len())
            .ok_or("native load overflow")?;
        target
            .get_mut(destination..end)
            .ok_or("native load outside emulated destination")?
            .copy_from_slice(&decoded.bytes);
        Ok(decoded.bytes)
    }
    pub(crate) fn load_raw(
        &mut self,
        _id: &str,
        offset: usize,
        length: usize,
        space: &str,
        destination: usize,
    ) -> Result<()> {
        let bytes = self
            .source
            .get(
                offset
                    ..offset
                        .checked_add(length)
                        .ok_or("native raw span overflow")?,
            )
            .ok_or("native raw span outside payload")?;
        let target = match space {
            "vram" => &mut self.vram,
            "bgPalette" => &mut self.bg_palette,
            "objPalette" => &mut self.obj_palette,
            _ => return Err("unknown native destination".into()),
        };
        let end = destination
            .checked_add(length)
            .ok_or("native raw destination overflow")?;
        target
            .get_mut(destination..end)
            .ok_or("native raw load outside destination")?
            .copy_from_slice(bytes);
        Ok(())
    }
    fn sprite(
        &self,
        tile: u16,
        bank: u8,
        width: usize,
        height: usize,
    ) -> Result<graphics::RenderedMap> {
        if !width.is_multiple_of(8) || !height.is_multiple_of(8) {
            return Err("unaligned native OBJ dimensions".into());
        }
        if bank >= 16
            || width == 0
            || height == 0
            || usize::from(tile)
                .checked_add(width * height / 64)
                .is_none_or(|end| end > 1024)
        {
            return Err("native OBJ tile span exceeds1024tile source space".into());
        }
        let tiles = graphics::decode_4bpp_tiles(&self.vram[0x10000..0x18000], 1024)?;
        let entries = (0..width * height / 64)
            .map(|i| graphics::TilemapEntry::from_word(tile + i as u16 + (u16::from(bank) << 12)))
            .collect::<Vec<_>>();
        let palette = graphics::parse_palette(&self.obj_palette, 256)?;
        Ok(graphics::render_text_map(
            &tiles,
            &entries,
            &palette,
            graphics::RenderOptions {
                width_tiles: width / 8,
                height_tiles: height / 8,
                depth: graphics::TileDepth::FourBpp,
                transparent_zero: true,
                max_pixels: 4096,
            },
        )?)
    }
    fn bg(
        &self,
        entries: &[u16],
        width: usize,
        height: usize,
        transparent: bool,
    ) -> Result<graphics::RenderedMap> {
        let tiles = graphics::decode_4bpp_tiles(&self.vram[..0x8000], 1024)?;
        let palette = graphics::parse_palette(&self.bg_palette, 256)?;
        let entries = entries
            .iter()
            .map(|&word| graphics::TilemapEntry::from_word(word))
            .collect::<Vec<_>>();
        Ok(graphics::render_text_map(
            &tiles,
            &entries,
            &palette,
            graphics::RenderOptions {
                width_tiles: width,
                height_tiles: height,
                depth: graphics::TileDepth::FourBpp,
                transparent_zero: transparent,
                max_pixels: 240 * 160,
            },
        )?)
    }
    fn screen(&self, offset: usize) -> Result<graphics::RenderedMap> {
        self.screen_depth(offset, graphics::TileDepth::FourBpp)
    }
    fn screen_depth(
        &self,
        offset: usize,
        depth: graphics::TileDepth,
    ) -> Result<graphics::RenderedMap> {
        self.map_image(offset, 30, 20, depth)
    }
    fn map_image(
        &self,
        offset: usize,
        width: usize,
        height: usize,
        depth: graphics::TileDepth,
    ) -> Result<graphics::RenderedMap> {
        let entries = (0..height)
            .flat_map(|y| {
                (0..width).map(move |x| {
                    u16::from_le_bytes(
                        self.vram[offset + y * 64 + x * 2..offset + y * 64 + x * 2 + 2]
                            .try_into()
                            .unwrap(),
                    )
                })
            })
            .collect::<Vec<_>>();
        if depth == graphics::TileDepth::FourBpp {
            return self.bg(&entries, width, height, true);
        }
        let tiles = graphics::decode_8bpp_tiles(&self.vram[..0x10000], 1024)?;
        let palette = graphics::parse_palette(&self.bg_palette, 256)?;
        let entries = entries
            .into_iter()
            .map(graphics::TilemapEntry::from_word)
            .collect::<Vec<_>>();
        Ok(graphics::render_text_map(
            &tiles,
            &entries,
            &palette,
            graphics::RenderOptions {
                width_tiles: width,
                height_tiles: height,
                depth,
                transparent_zero: true,
                max_pixels: 240 * 512,
            },
        )?)
    }
}
fn blit(
    target: &mut graphics::RenderedMap,
    source: &graphics::RenderedMap,
    x: usize,
    y: usize,
) -> Result<()> {
    if x + source.width > target.width || y + source.height > target.height {
        return Err("native composition exceeds canvas".into());
    }
    for sy in 0..source.height {
        for sx in 0..source.width {
            let from = (sy * source.width + sx) * 4;
            let to = ((y + sy) * target.width + x + sx) * 4;
            if source.rgba[from + 3] != 0 {
                target.rgba[to..to + 4].copy_from_slice(&source.rgba[from..from + 4]);
            }
        }
    }
    Ok(())
}
/// Draw at signed screen coordinates, clipping transparent and off-screen texels.
pub(crate) fn blit_clipped(target: &mut RenderedMap, source: &RenderedMap, x: i32, y: i32) {
    for row in 0..source.height {
        for col in 0..source.width {
            let dx = i64::from(x) + col as i64;
            let dy = i64::from(y) + row as i64;
            if dx < 0 || dy < 0 || dx >= target.width as i64 || dy >= target.height as i64 {
                continue;
            }
            let i = (row * source.width + col) * 4;
            let j = (dy as usize * target.width + dx as usize) * 4;
            if source.rgba[i + 3] != 0 {
                target.rgba[j..j + 4].copy_from_slice(&source.rgba[i..i + 4]);
            }
        }
    }
}
fn flip(image: &mut graphics::RenderedMap, horizontal: bool, vertical: bool) {
    if !horizontal && !vertical {
        return;
    }
    let before = image.rgba.clone();
    for y in 0..image.height {
        for x in 0..image.width {
            let sx = if horizontal { image.width - 1 - x } else { x };
            let sy = if vertical { image.height - 1 - y } else { y };
            let from = (sy * image.width + sx) * 4;
            let to = (y * image.width + x) * 4;
            image.rgba[to..to + 4].copy_from_slice(&before[from..from + 4]);
        }
    }
}
/// A2NE 02001088 emits each native8-byte descriptor as ordinary 4bpp OAM.
/// Retain its tile offset, shape/size, per-piece flips and signed anchor.
pub(crate) fn descriptor_styled(
    scene: &Scene<'_>,
    offset: usize,
    tile: u16,
    style: [u8; 3],
) -> Result<(graphics::RenderedMap, [i32; 2], bool)> {
    const SIZES: [[(usize, usize); 4]; 3] = [
        [(8, 8), (16, 16), (32, 32), (64, 64)],
        [(16, 8), (32, 8), (32, 16), (64, 32)],
        [(8, 16), (8, 32), (16, 32), (32, 64)],
    ];
    let mut pieces = Vec::new();

    for n in 0..16 {
        let at = offset + n * 8;
        let row = scene
            .source
            .get(at..at + 8)
            .ok_or("truncated sprite descriptor")?;
        let attr0 = row[2] | style[0];
        let attr1 = row[3] | style[1];
        let attr2 = row[4] | style[2];
        if attr0 & 0x20 != 0 {
            return Err("unmapped8bpp OBJ descriptor".into());
        }
        let affine = attr0 & 1 != 0;
        if !affine && attr1 & 15 != 0 {
            return Err("unmapped non-affine descriptor flags".into());
        }
        let shape = usize::from(attr0 >> 6);
        let size = usize::from(attr1 >> 6);
        let (width, height) = *SIZES
            .get(shape)
            .and_then(|r| r.get(size))
            .ok_or("invalid OBJ shape")?;
        let bank = attr2 >> 4;
        let base = tile
            .checked_add(u16::from_le_bytes(row[..2].try_into()?))
            .ok_or("descriptor tile overflow")?;
        let mut image = scene.sprite(base, bank, width, height)?;
        let anchor = [i32::from(row[5] as i8), i32::from(row[6] as i8)];
        if affine {
            if n != 0 || row[7] != 0 {
                return Err("unmapped multi-piece affine descriptor".into());
            }
            return Ok((image, anchor, true));
        }
        flip(&mut image, attr1 & 0x10 != 0, attr1 & 0x20 != 0);
        pieces.push((image, i32::from(row[5] as i8), i32::from(row[6] as i8)));
        if row[7] == 0 {
            break;
        }
        if n == 15 {
            return Err("unterminated sprite descriptor".into());
        }
    }
    let minx = pieces.iter().map(|p| p.1).min().ok_or("empty descriptor")?;
    let miny = pieces.iter().map(|p| p.2).min().ok_or("empty descriptor")?;
    let maxx = pieces.iter().map(|p| p.1 + p.0.width as i32).max().unwrap();
    let maxy = pieces
        .iter()
        .map(|p| p.2 + p.0.height as i32)
        .max()
        .unwrap();
    let width = usize::try_from(maxx - minx)?;
    let height = usize::try_from(maxy - miny)?;
    let mut output = graphics::RenderedMap {
        width,
        height,
        rgba: vec![0; width * height * 4],
    };
    for (image, x, y) in pieces.iter().rev() {
        blit(
            &mut output,
            image,
            usize::try_from(x - minx)?,
            usize::try_from(y - miny)?,
        )?;
    }
    Ok((output, [minx, miny], false))
}
fn template(
    scene: &mut Scene<'_>,
    offset: usize,
    dest: usize,
    width: usize,
    height: usize,
    base: u16,
) -> Result<()> {
    let length = width
        .checked_mul(height)
        .and_then(|v| v.checked_mul(2))
        .ok_or("native template overflow")?;
    let bytes = scene
        .source
        .get(offset..offset + length)
        .ok_or("native template outside source")?;
    for y in 0..height {
        for x in 0..width {
            let at = (y * width + x) * 2;
            let word = u16::from_le_bytes(bytes[at..at + 2].try_into()?).wrapping_add(base);
            let dst = dest + y * 64 + x * 2;
            scene
                .vram
                .get_mut(dst..dst + 2)
                .ok_or("template outside VRAM")?
                .copy_from_slice(&word.to_le_bytes());
        }
    }
    Ok(())
}
fn backdrop(scene: &Scene<'_>) -> Result<graphics::RenderedMap> {
    backdrop_dimensions(scene, 240, 160)
}
fn backdrop_dimensions(
    scene: &Scene<'_>,
    width: usize,
    height: usize,
) -> Result<graphics::RenderedMap> {
    let color = graphics::PaletteColor::from_word(u16::from_le_bytes(
        scene.bg_palette[..2].try_into().unwrap(),
    ))
    .rgb8();
    let mut out = graphics::RenderedMap {
        width,
        height,
        rgba: vec![0; width * height * 4],
    };
    for pixel in out.rgba.as_chunks_mut::<4>().0 {
        pixel[..3].copy_from_slice(&color);
        pixel[3] = 255;
    }
    Ok(out)
}

fn memory_parts(source: &[u8]) -> Result<(Scene<'_>, RenderedMap)> {
    let mut scene = Scene::new(source);
    for (id, offset, space, dest) in [
        ("bg-palette", 0x13d58, "bgPalette", 0),
        ("bg-common", 0x128a8, "vram", 0),
        ("bg-memory", 0x13e50, "vram", 0x1300),
        ("bg-english", 0x134c8, "vram", 0xf00),
        ("bg-memory-english", 0x14210, "vram", 0x19a0),
        ("obj-palette", 0x144d8, "objPalette", 0x40),
        ("obj-common", 0x13620, "vram", 0x10000),
        ("obj-cards", 0x14574, "vram", 0x10de0),
    ] {
        scene.load(id, offset, space, dest)?;
    }
    // Memory replaces OBJ banks 2..7; banks 0 and 1 survive from InitGarden.
    scene.load_raw("garden-shared", 0x246e4, 64, "objPalette", 0)?;
    scene.load_raw("normal-chao", 0x248e4, 32, "objPalette", 0)?;
    let template = &source[0x15e10..0x15e20];
    let mut board = vec![0_u16; 32 * 32];
    for row in 0_usize..5 {
        for col in 0_usize..6 {
            let base = if (1..=3).contains(&row) && (1..=4).contains(&col) {
                if row == 2 && (2..=3).contains(&col) {
                    0xaa
                } else {
                    0xa1
                }
            } else {
                0x98
            };
            for y in 0..4 {
                for x in 0..4 {
                    board[(row * 4 + y) * 32 + col * 4 + x] = base + u16::from(template[y * 4 + x]);
                }
            }
        }
    }
    for (i, &word) in board.iter().enumerate() {
        scene.vram[0xd800 + i * 2..0xd802 + i * 2].copy_from_slice(&word.to_le_bytes());
    }
    for (_id, offset, dest) in [
        ("sidebar-map", 0x14408, 0xd030),
        ("sidebar-overlay", 0x14478, 0xc830),
    ] {
        let decoded = native_lz(source, offset)?;
        if decoded.bytes.len() != 240 {
            return Err("native Memory sidebar map length mismatch".into());
        }
        for row in 0..20 {
            scene.vram[dest + row * 64..dest + row * 64 + 12]
                .copy_from_slice(&decoded.bytes[row * 12..row * 12 + 12]);
        }
    }
    let mut bg = backdrop(&scene)?;
    for offset in [0xd800, 0xd000, 0xc800] {
        blit(&mut bg, &scene.screen(offset)?, 0, 0)?;
    }
    Ok((scene, bg))
}
pub fn memory(
    rom: &VerifiedRom<'_>,
    view: &NativeMemoryView,
) -> std::result::Result<RenderedMap, crate::Error> {
    memory_frame(rom.garden_bytes(), view).map_err(|e| crate::Error(e.to_string()))
}
fn memory_frame(source: &[u8], view: &NativeMemoryView) -> Result<RenderedMap> {
    let (scene, mut board) = memory_parts(source)?;
    for (i, c) in view.cells.iter().enumerate().filter(|(_, c)| c.occupied) {
        let (tile, bank) = match c.face {
            None => (0x6f, 5),
            Some(f @ 0..=6) => (0x7f + 16 * u16::from(f), 2 + (f >> 1)),
            _ => return Err("invalid card face".into()),
        };
        blit(
            &mut board,
            &scene.sprite(tile, bank, 32, 32)?,
            i % 6 * 32,
            i / 6 * 32,
        )?;
    }
    memory_overlays(&scene, view, &mut board)?;
    Ok(board)
}
fn memory_overlays(
    scene: &Scene<'_>,
    view: &NativeMemoryView,
    board: &mut RenderedMap,
) -> Result<()> {
    if let Some(w) = view.walker.filter(|w| w.trip != 1) {
        let base = match w.direction {
            0 => 27,
            1 => 0,
            2 | 3 => 54,
            _ => return Err("invalid walker direction".into()),
        };
        let stride = match (w.animation_frame.wrapping_sub(1) >> 3) & 3 {
            1 => 9,
            3 => 18,
            _ => 0,
        };
        let x = i32::from(w.x);
        let y = i32::from(w.y);
        // 020099f8 sets tiles; 02009a70 positions native OAM slots 6..10.
        // Draw in descending OAM index so lower index wins overlaps.
        for (tile, width, height, dx, dy) in [
            (108, 8, 8, if w.mirror { 4 } else { 12 }, -8),
            (base + stride + 8, 8, 8, if w.mirror { 0 } else { 16 }, 16),
            (base + stride + 6, 16, 8, if w.mirror { 8 } else { 0 }, 16),
            (base + stride + 4, 8, 16, if w.mirror { 0 } else { 16 }, 0),
            (base + stride, 16, 16, if w.mirror { 8 } else { 0 }, 0),
        ] {
            let mut sprite = scene.sprite(tile, 0, width, height)?;
            flip(&mut sprite, w.mirror, false);
            blit_clipped(board, &sprite, x + dx, y + dy);
        }
    }
    if let Some((col, row)) = view.cursor {
        if col >= 6 || row >= 5 {
            return Err("invalid Memory cursor cell".into());
        }
        let oam = native_lz(scene.source, 0x14d6c)?.bytes;
        let attr2 = u16::from_le_bytes(oam[4..6].try_into()?);
        let cursor = scene.sprite(attr2 & 1023, (attr2 >> 12) as u8, 16, 16)?;
        // 0200a638 settled unpressed target. Jump immediately to the actual
        // input cell in a step-by-step inspection instead of lagging behind it.
        blit_clipped(
            board,
            &cursor,
            i32::from(col) * 32 + 20,
            (i32::from(row) * 32 - 8).max(0),
        );
    }
    Ok(())
}
fn bounce_parts(source: &[u8]) -> Result<(Scene<'_>, RenderedMap, RenderedMap)> {
    let mut scene = Scene::new(source);
    scene.load_raw("bg-palette", 0x17524, 0x140, "bgPalette", 0)?;
    scene.load_raw("obj-palette", 0x15ee0, 0xe0, "objPalette", 0)?;
    for (id, offset, dest) in [
        ("bg-common", 0x17664, 0x40),
        ("bg-decoration", 0x188d8, 0x8000),
        ("obj-tiles", 0x15fc0, 0x10000),
        ("obj-extra", 0x199d0, 0x17ee0),
    ] {
        scene.load(id, offset, "vram", dest)?;
    }
    // D2F0 clears sixteen decoded8bpp tiles immediately after the base tiles.
    scene.vram[0x1c40..0x1d40].fill(0);
    for (offset, dest, width, height, base) in [
        (0x2eb4a, 0xc0b4, 3, 2, 0x8000),
        (0x2eb56, 0xc1f8, 1, 2, 0x8000),
        (0x2e806, 0xc830, 6, 20, 0x809c),
        (0x2e90e, 0xc9b2, 2, 4, 0x90da),
        (0x2e8f6, 0xc830, 6, 2, 0x809c),
    ] {
        template(&mut scene, offset, dest, width, height, base)?;
    }
    let decoded = native_lz(source, 0x18624)?;
    if decoded.bytes.len() != 24 * 40 * 2 {
        return Err("native Bounce world map length mismatch".into());
    }
    for row in 0..40 {
        for col in 0..24 {
            let at = (row * 24 + col) * 2;
            let word =
                u16::from_le_bytes(decoded.bytes[at..at + 2].try_into()?).wrapping_add(0x200);
            let dst = 0xd000 + row * 64 + col * 2;
            scene.vram[dst..dst + 2].copy_from_slice(&word.to_le_bytes());
        }
    }
    let mut world = backdrop_dimensions(&scene, 240, 320)?;
    blit(
        &mut world,
        &scene.map_image(0xd000, 30, 40, graphics::TileDepth::EightBpp)?,
        0,
        0,
    )?;
    let mut hud = graphics::RenderedMap {
        width: 240,
        height: 160,
        rgba: vec![0; 240 * 160 * 4],
    };
    for offset in [0xc800, 0xc000, 0xb800] {
        blit(&mut hud, &scene.screen(offset)?, 0, 0)?;
    }
    Ok((scene, world, hud))
}
pub fn bounce(
    rom: &VerifiedRom<'_>,
    view: &NativeBounceView,
) -> std::result::Result<RenderedMap, crate::Error> {
    bounce_frame(rom.garden_bytes(), view).map_err(|e| crate::Error(e.to_string()))
}
fn bounce_frame(source: &[u8], v: &NativeBounceView) -> Result<RenderedMap> {
    let (scene, world, hud) = bounce_parts(source)?;
    let y = usize::try_from(v.scroll)?
        .checked_mul(240 * 4)
        .ok_or("invalid scroll")?;
    let mut frame = RenderedMap {
        width: 240,
        height: 160,
        rgba: world
            .rgba
            .get(y..y + 240 * 160 * 4)
            .ok_or("scroll outside world")?
            .to_vec(),
    };
    let mut draw = |a: &BounceActorView, transform: Option<BounceAffineView>| -> Result<()> {
        if !a.visible {
            return Ok(());
        }
        let Some(offset) = a.descriptor_offset else {
            return Ok(());
        };
        let (sprite, anchor, affine) = descriptor_styled(
            &scene,
            offset as usize,
            a.tile,
            [a.style, a.transform, a.packed_palette_layer],
        )?;
        // ObjAffineSet is exact at zero angle. Rotated matrices require BIOS
        // sine values; skip unsupported poses instead of drawing them untransformed.
        let (mx, my, double) = if affine {
            let Some(m) = transform else { return Ok(()) };
            if m.rotation >> 8 != 0 {
                return Ok(());
            }
            (
                i32::from(m.scale_x),
                i32::from(m.scale_y),
                ((scene.source[offset as usize + 2] | a.style) & 2) != 0,
            )
        } else {
            (256, 256, false)
        };
        let sw = sprite.width as i32;
        let sh = sprite.height as i32;
        let width = if double { sw * 2 } else { sw };
        let height = if double { sh * 2 } else { sh };
        for dy in 0..height {
            for dx in 0..width {
                let x = i32::from(a.x) + anchor[0] + dx;
                let y = i32::from(a.y) + anchor[1] + dy;
                if !(0..240).contains(&x) || !(0..160).contains(&y) {
                    continue;
                }
                let sx = (((dx - width / 2) * mx) >> 8) + sw / 2;
                let sy = (((dy - height / 2) * my) >> 8) + sh / 2;
                if !(0..sw).contains(&sx) || !(0..sh).contains(&sy) {
                    continue;
                }
                let i = (sy as usize * sprite.width + sx as usize) * 4;
                let j = (y as usize * 240 + x as usize) * 4;
                if sprite.rgba[i + 3] != 0 {
                    frame.rgba[j..j + 4].copy_from_slice(&sprite.rgba[i..i + 4]);
                }
            }
        }
        Ok(())
    };
    for a in &v.clouds {
        draw(a, None)?
    }
    for a in v.targets.iter().flatten() {
        draw(a, None)?
    }
    for (a, m) in v.bumpers.iter().zip(v.bumper_affines) {
        if let Some(a) = a {
            draw(a, m)?
        }
    }
    for p in &v.spring_parts {
        draw(&p.actor, p.affine)?
    }
    draw(&v.chao, None)?;
    blit(&mut frame, &hud, 0, 0)?;
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires verified local A2NE_ROM"]
    fn cartridge_pixels_match_thumb_verified_presentation() {
        let raw = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
        let rom = VerifiedRom::parse(&raw).unwrap();
        let (m, board) = memory_parts(rom.garden_bytes()).unwrap();
        let (b, world, hud) = bounce_parts(rom.garden_bytes()).unwrap();
        assert_eq!(
            gba::sha256(&board.rgba),
            "39aa540d4c4770111baf5d0334ddbb7e6914f6306fcdcb3ff6a77cb0fd1f8542"
        );
        assert_eq!(
            gba::sha256(&world.rgba),
            "282a1b494bdcc56724858c0c9a7c380b1556233c34105ee444f74ad111d936c0"
        );
        assert_eq!(
            gba::sha256(&hud.rgba),
            "3e7528fe238849d30bf1b4c3a5530462e0aa2feab452fab9d0d2f397fd2f9c58"
        );
        assert_eq!(
            gba::sha256(&m.sprite(111, 5, 32, 32).unwrap().rgba),
            "f4744d1fe1b680d48a0adbcbbf9c2292f2776e94d75ef9b4d277455eb6f9bab8"
        );
        assert_eq!(
            gba::sha256(&m.sprite(127, 2, 32, 32).unwrap().rgba),
            "9a4b920847fd391a4384784e1c14847f028b5d9a21cf3088ef100c2f5c857aa7"
        );
        assert_eq!(
            gba::sha256(&m.sprite(143, 2, 32, 32).unwrap().rgba),
            "f78eed1be1d5507564312cba8aa652d916ca16f950bd2791e9998107387bc4e8"
        );
        assert_eq!(
            gba::sha256(&m.sprite(159, 3, 32, 32).unwrap().rgba),
            "7be80d3c5b43f938f252f72b7ebedacfc4b534df87c83315d1e4b21cbdaf5325"
        );
        assert_eq!(
            gba::sha256(&m.sprite(175, 3, 32, 32).unwrap().rgba),
            "1f445a8c6798657e765a47cdb5e566c3311131604eb52dc013aba6637a98833c"
        );
        assert_eq!(
            gba::sha256(&m.sprite(191, 4, 32, 32).unwrap().rgba),
            "ea3fe1a2a0a3d0c83f22728eb464643585ed65f7a0e4f2f637de68e72e38156f"
        );
        assert_eq!(
            gba::sha256(&m.sprite(207, 4, 32, 32).unwrap().rgba),
            "fa8dfccc7b0e9f592917732ceb917ff33e2242830c5a9d77b3017f056f58e49b"
        );
        assert_eq!(
            gba::sha256(&m.sprite(223, 5, 32, 32).unwrap().rgba),
            "c9adf6786562adaef08fb62af77884b3b177c10f8dc5ccba090e0f9b042e71ca"
        );
        for (offset, tile, style, digest) in [
            (
                0x2e4c8,
                310,
                [0, 0, 12],
                "aa0ab55d315341a3a28b01c4516a63152b3ff7dd40d860cba6ad106e213cfc69",
            ),
            (
                0x2e4c8,
                314,
                [0, 0, 12],
                "c1933e36f9f382acdb5586048aba9cb0ac9775d945c6d1513dd0a695a3551c2b",
            ),
            (
                0x2e4c8,
                318,
                [0, 0, 12],
                "de64f98d3a78bf2f8d68a3ceeb65d248f27150502f6feecfd1d52294b940fc32",
            ),
            (
                0x2e4c8,
                322,
                [0, 0, 12],
                "d5539c1bdd41d0bfca8a2eaaa20fc86659880b7bc6def150f6a5b14f7d8c8cc0",
            ),
            (
                0x2e4d0,
                310,
                [0, 0, 12],
                "1d935711b35d80fbfad16cde8ae646e7f70d9c5035323fca1c8a1feb6ea4458c",
            ),
            (
                0x2e4d0,
                314,
                [0, 0, 12],
                "8a41d0e34e62943d6911eefbaef8139779de6ab04458adcdca87f8a885a56271",
            ),
            (
                0x2e4d0,
                318,
                [0, 0, 12],
                "3e43a478a8d2517d89d32d900bbf3a302a4513fcade9b7ed9217e2f89b4977c2",
            ),
            (
                0x2e4d0,
                322,
                [0, 0, 12],
                "a6aa8e5fee0b078aea5766cd5148462400614ae69ae519758c74c0a25e65cf1d",
            ),
            (
                0x2e4d8,
                326,
                [0, 0, 12],
                "5f49dfe8c6c3ccb6dfe712b59565a543945d05cf191d7af71948979992dd2c1d",
            ),
            (
                0x2e4d8,
                330,
                [0, 0, 12],
                "45db8e2b9edda3500f95b52d20399687174c60638c49c679b67b62eb8d058b10",
            ),
            (
                0x2e4d8,
                334,
                [0, 0, 12],
                "5894bc63c3c8ddbe790bc124ed5ae77b38b22a44e4b4fd18811a81e758d9f032",
            ),
            (
                0x2e4e0,
                326,
                [0, 0, 12],
                "c04f0531bf50ddb7b2957d016fab0f91a26028adfc657e58f98ba4bc80f51d63",
            ),
            (
                0x2e4e0,
                330,
                [0, 0, 12],
                "d5a88ec28065604f838b39dd786c807c8c6f9f62d8eb948f0a71c8bbb6099c03",
            ),
            (
                0x2e4e0,
                334,
                [0, 0, 12],
                "646af07464d70eab42caa6e5674375073ddf98cf0ceb7682a22263fbc3262955",
            ),
            (
                0x2e4e8,
                338,
                [0, 0, 12],
                "ddb9bdffd8b1f1f0f227ea6e87ba2778b35969870ae92e994c1098801ffdeb34",
            ),
            (
                0x2e4e8,
                346,
                [0, 0, 12],
                "f7caacb915ecca55b8e3a68cde5efc2253424aa9475139bc005b09b2b375fc44",
            ),
            (
                0x2e4f0,
                338,
                [0, 0, 12],
                "6dea364c33792774f2bdc8b7f542be623e6a1c1ffd3416fda470381e0ca0d96a",
            ),
            (
                0x2e4f8,
                0,
                [0, 0, 12],
                "242b53cddd5805dfd3a4f9acba62d67db6615eee8b0563615bf15ae56800c779",
            ),
            (
                0x2e4f8,
                4,
                [0, 0, 12],
                "c10db6b2366772bca91df3156d4c8ea3938ff80a34a6ea476029c7d938f068c3",
            ),
            (
                0x2e4f8,
                8,
                [0, 0, 12],
                "6bd7ad89429ee237c9043d58dce22e0eda8effc8587ef44bec14878b185627aa",
            ),
            (
                0x2e4f8,
                12,
                [0, 0, 12],
                "e784ae1b7b1ff0482af7904124017823c9b3e30635795c647139db7c619a9303",
            ),
            (
                0x2e4f8,
                16,
                [0, 0, 12],
                "8b9a0baca0db81e5e02cda03c7e5087cb3a73904408829e3f6580c4809ea30ec",
            ),
            (
                0x2e4f8,
                20,
                [0, 0, 12],
                "d8692643486f46aab39051a5c7b46fa86c6645edd69fe2a7dd70a1e78f9bbf2f",
            ),
            (
                0x2e4f8,
                24,
                [0, 0, 12],
                "07d295ce7b1d1b79442ab7a73cb12b5de9a2b39d0c13b343e7df76eb340f10c6",
            ),
            (
                0x2e4f8,
                28,
                [0, 0, 12],
                "eed49366e724a07b48be09f34ebac05440622da1d494464e094f1d947c97b5bd",
            ),
            (
                0x2e4f8,
                32,
                [0, 0, 12],
                "2b356acf3a3c6338ed4b7293be142cf585299da69311968cfb2e0be6ac22dfbf",
            ),
            (
                0x2e4f8,
                32,
                [0, 16, 12],
                "46d6e3dd71532ba1fbcedc8185ecdbd1bab00c67f782a3a2bf8bba2ab53a721a",
            ),
            (
                0x2e4f8,
                36,
                [0, 0, 12],
                "46ed58461591268b10f5b7d9720dd25d5237842cfb0f111485b0bb4f3f702c3a",
            ),
            (
                0x2e4f8,
                36,
                [0, 16, 12],
                "849278ea3993b0b53ffafea32d4f84dc051b38d01c58c92572536c89dc23f250",
            ),
            (
                0x2e4f8,
                40,
                [0, 16, 12],
                "e65446947f0e098a5d22b2dba02530aa97ee93cb25a8124eac3149d938fac34a",
            ),
            (
                0x2e500,
                0,
                [0, 0, 44],
                "427ed09a99be9da234b0b2ac8975147b75134ce3492133b13b0fc038682ad136",
            ),
            (
                0x2e500,
                0,
                [4, 0, 44],
                "427ed09a99be9da234b0b2ac8975147b75134ce3492133b13b0fc038682ad136",
            ),
            (
                0x2e508,
                0,
                [1, 0, 44],
                "0bfd48b08fc0d05db426d7f4ba3fb8318eac900ce1cf0ab6200309ae6d2f6d81",
            ),
            (
                0x2e508,
                0,
                [5, 0, 44],
                "0bfd48b08fc0d05db426d7f4ba3fb8318eac900ce1cf0ab6200309ae6d2f6d81",
            ),
            (
                0x2e510,
                0,
                [0, 0, 44],
                "5b2caa5c2a84b40cb9277f6e48b4f256f3fc4e0b30e7784bed1023e9a2b44874",
            ),
            (
                0x2e510,
                0,
                [4, 0, 44],
                "5b2caa5c2a84b40cb9277f6e48b4f256f3fc4e0b30e7784bed1023e9a2b44874",
            ),
            (
                0x2e520,
                0,
                [0, 0, 44],
                "525d9de36108c594c7501b3cdb468490b301d9ed289570e2c76e8552be50d0c3",
            ),
            (
                0x2e520,
                0,
                [4, 0, 44],
                "525d9de36108c594c7501b3cdb468490b301d9ed289570e2c76e8552be50d0c3",
            ),
            (
                0x2e530,
                0,
                [0, 0, 44],
                "a066fa8e96a88be1202ee4f693800fa63e5125d8d5e26f167ca0b314024c39f2",
            ),
            (
                0x2e530,
                0,
                [4, 0, 44],
                "a066fa8e96a88be1202ee4f693800fa63e5125d8d5e26f167ca0b314024c39f2",
            ),
            (
                0x2e530,
                4,
                [0, 0, 44],
                "561dd30da2a325c72e2ba81194a97142401c0b65153bce1eb73ea47455ce6272",
            ),
            (
                0x2e530,
                4,
                [4, 0, 44],
                "561dd30da2a325c72e2ba81194a97142401c0b65153bce1eb73ea47455ce6272",
            ),
            (
                0x2e530,
                8,
                [0, 0, 44],
                "ace17f5d8492108d3b74f7a8a94b2776ef6a871c1fc9d434814053d898fdfb2a",
            ),
            (
                0x2e530,
                8,
                [4, 0, 44],
                "ace17f5d8492108d3b74f7a8a94b2776ef6a871c1fc9d434814053d898fdfb2a",
            ),
            (
                0x2e540,
                102,
                [0, 0, 60],
                "b7514f45d9b08456c05316f932ba7cef7300b091fac4627a117ddc4d2004284f",
            ),
            (
                0x2e540,
                102,
                [4, 0, 60],
                "b7514f45d9b08456c05316f932ba7cef7300b091fac4627a117ddc4d2004284f",
            ),
            (
                0x2e540,
                118,
                [0, 0, 60],
                "b68f4dc83c8b18ae397b82d4665b2e6ab8f541dd0a50eafbbf95304dda3add0c",
            ),
            (
                0x2e540,
                118,
                [4, 0, 60],
                "b68f4dc83c8b18ae397b82d4665b2e6ab8f541dd0a50eafbbf95304dda3add0c",
            ),
            (
                0x2e540,
                134,
                [0, 0, 60],
                "70348e053a25c933db918dca6e83336c5731bc8b34135f0f558f0ccd1b7fd80e",
            ),
            (
                0x2e540,
                134,
                [4, 0, 60],
                "70348e053a25c933db918dca6e83336c5731bc8b34135f0f558f0ccd1b7fd80e",
            ),
            (
                0x2e540,
                150,
                [0, 0, 60],
                "42df6a45d0fdac2c217ad15c5c8b44a94357f124bf0bacbdbab9beb08ebcf906",
            ),
            (
                0x2e540,
                150,
                [4, 0, 60],
                "42df6a45d0fdac2c217ad15c5c8b44a94357f124bf0bacbdbab9beb08ebcf906",
            ),
            (
                0x2e540,
                166,
                [0, 0, 60],
                "0aae992ef436112445c24a9fba626848dc458efab91b91302d83057915b06b8a",
            ),
            (
                0x2e540,
                166,
                [4, 0, 60],
                "0aae992ef436112445c24a9fba626848dc458efab91b91302d83057915b06b8a",
            ),
            (
                0x2e540,
                182,
                [0, 0, 60],
                "f8498d029017b2aa86e55f1f9cadaf50551492dc04745d1944c337a165e6370a",
            ),
            (
                0x2e540,
                182,
                [4, 0, 60],
                "f8498d029017b2aa86e55f1f9cadaf50551492dc04745d1944c337a165e6370a",
            ),
            (
                0x2e540,
                198,
                [0, 0, 60],
                "663d0f0b9e29318eafa780b77dfa3300a43cb2c6db72332be72fcbea493fde2f",
            ),
            (
                0x2e540,
                198,
                [4, 0, 60],
                "663d0f0b9e29318eafa780b77dfa3300a43cb2c6db72332be72fcbea493fde2f",
            ),
            (
                0x2e540,
                214,
                [1, 2, 60],
                "b2d2d4c8d412c04c2b00ba47c3957019f62a5bcd620e5ff5c4920ef11cf2c670",
            ),
            (
                0x2e540,
                214,
                [1, 4, 60],
                "b2d2d4c8d412c04c2b00ba47c3957019f62a5bcd620e5ff5c4920ef11cf2c670",
            ),
            (
                0x2e540,
                214,
                [1, 6, 60],
                "b2d2d4c8d412c04c2b00ba47c3957019f62a5bcd620e5ff5c4920ef11cf2c670",
            ),
            (
                0x2e540,
                214,
                [5, 2, 60],
                "b2d2d4c8d412c04c2b00ba47c3957019f62a5bcd620e5ff5c4920ef11cf2c670",
            ),
            (
                0x2e540,
                214,
                [5, 4, 60],
                "b2d2d4c8d412c04c2b00ba47c3957019f62a5bcd620e5ff5c4920ef11cf2c670",
            ),
            (
                0x2e540,
                214,
                [5, 6, 60],
                "b2d2d4c8d412c04c2b00ba47c3957019f62a5bcd620e5ff5c4920ef11cf2c670",
            ),
            (
                0x2e540,
                230,
                [0, 0, 60],
                "5e9bde3cf3c7f79db2405f148f129d1573a8e45b5020c59750196267be145280",
            ),
            (
                0x2e540,
                230,
                [4, 0, 60],
                "5e9bde3cf3c7f79db2405f148f129d1573a8e45b5020c59750196267be145280",
            ),
            (
                0x2e540,
                246,
                [0, 0, 60],
                "76fc3a50453e3b57007da6b3baf87645a2a38aef66693ed105709739277982e5",
            ),
            (
                0x2e540,
                246,
                [0, 16, 60],
                "4cde9a3dfa4dcca308d1daa8a5be6b1cf1ac069c438953c066fd76dcdacf3e7c",
            ),
            (
                0x2e540,
                246,
                [4, 0, 60],
                "76fc3a50453e3b57007da6b3baf87645a2a38aef66693ed105709739277982e5",
            ),
            (
                0x2e540,
                246,
                [4, 16, 60],
                "4cde9a3dfa4dcca308d1daa8a5be6b1cf1ac069c438953c066fd76dcdacf3e7c",
            ),
            (
                0x2e540,
                262,
                [0, 0, 60],
                "200ff21b9162ef7cbed7b801550b3e84a149581f3104ad6e32f796cb80f89d2c",
            ),
            (
                0x2e540,
                262,
                [0, 16, 60],
                "9700b1a91fa91bc2ee14ab94b4f57beca850e71f7fa7dd51690aa1a7edc06c90",
            ),
            (
                0x2e540,
                262,
                [4, 0, 60],
                "200ff21b9162ef7cbed7b801550b3e84a149581f3104ad6e32f796cb80f89d2c",
            ),
            (
                0x2e540,
                262,
                [4, 16, 60],
                "9700b1a91fa91bc2ee14ab94b4f57beca850e71f7fa7dd51690aa1a7edc06c90",
            ),
            (
                0x2e540,
                278,
                [0, 0, 60],
                "4c3795b03d4744a33f818f378fe74e241bd39ded3a8c01378a9f4a8d33e4548f",
            ),
            (
                0x2e540,
                294,
                [0, 0, 60],
                "a45e86965dc8ed2c82a945c7fa8f8006ad0a7538b3f2698e298b817c74868c16",
            ),
            (
                0x2e548,
                230,
                [3, 2, 60],
                "9d7b5723f4b84bb449e19ab2463350f794b815f080d7200c88a1bf6e7783a9a8",
            ),
            (
                0x2e548,
                230,
                [3, 4, 60],
                "9d7b5723f4b84bb449e19ab2463350f794b815f080d7200c88a1bf6e7783a9a8",
            ),
            (
                0x2e548,
                230,
                [3, 6, 60],
                "9d7b5723f4b84bb449e19ab2463350f794b815f080d7200c88a1bf6e7783a9a8",
            ),
            (
                0x2e548,
                230,
                [7, 2, 60],
                "9d7b5723f4b84bb449e19ab2463350f794b815f080d7200c88a1bf6e7783a9a8",
            ),
            (
                0x2e548,
                230,
                [7, 4, 60],
                "9d7b5723f4b84bb449e19ab2463350f794b815f080d7200c88a1bf6e7783a9a8",
            ),
            (
                0x2e548,
                230,
                [7, 6, 60],
                "9d7b5723f4b84bb449e19ab2463350f794b815f080d7200c88a1bf6e7783a9a8",
            ),
            (
                0x2e550,
                0,
                [0, 2, 0],
                "baed9352ab603b856e7596cbd0594bac8bfae2632e148ecee0670e255ffda50d",
            ),
            (
                0x2e550,
                0,
                [0, 4, 0],
                "baed9352ab603b856e7596cbd0594bac8bfae2632e148ecee0670e255ffda50d",
            ),
            (
                0x2e558,
                60,
                [0, 0, 28],
                "3d52372edd3399c9967300db0bf4958eb2cc4084b8962b59ad961e307e8c8c81",
            ),
        ] {
            assert_eq!(
                gba::sha256(&descriptor_styled(&b, offset, tile, style).unwrap().0.rgba),
                digest
            );
        }

        let round = crate::native_garden_memory::A2neMemorySource::from_verified_payload(
            rom.garden_bytes(),
        )
        .unwrap()
        .begin_round(0)
        .unwrap();
        let mut v = round.view();
        assert_eq!(
            gba::sha256(&m.sprite(0x107, 1, 16, 16).unwrap().rgba),
            "0c4c57f09629cc36b785d4f1144eae01df97109b9858e14d8001e238b2118666"
        );
        let blank = || RenderedMap {
            width: 240,
            height: 160,
            rgba: vec![0; 240 * 160 * 4],
        };
        let mut cursor = Vec::new();
        for row in 0..5 {
            for col in 0..6 {
                v.cursor = Some((col, row));
                let mut frame = blank();
                memory_overlays(&m, &v, &mut frame).unwrap();
                cursor.extend(frame.rgba);
            }
        }
        assert_eq!(
            gba::sha256(&cursor),
            "c6c213f7dec093d3075a129f81bc0e8fd1adafc2306e5dd333602af24a159862"
        );
        v.cursor = None;
        let mut walk = Vec::new();
        for mirror in [false, true] {
            for direction in [1, 0, 2] {
                for animation_frame in [1, 9, 25] {
                    v.walker = Some(crate::native_garden_memory::VisibleMemoryWalker {
                        x: 72,
                        y: 48,
                        direction,
                        trip: 0,
                        moving_position: None,
                        animation_frame,
                        mirror,
                    });
                    let mut frame = blank();
                    memory_overlays(&m, &v, &mut frame).unwrap();
                    walk.extend(frame.rgba);
                }
            }
        }
        assert_eq!(
            gba::sha256(&walk),
            "519fb3a00e44e9a7a0dc3e3acf7139cc78ef9eb65276b0cd2b68cb7a817829cc"
        );
        v.walker = None;
        let placed = memory(&rom, &v).unwrap();
        assert_ne!(placed, board);
        let i = v.cells.iter().position(|c| c.occupied).unwrap();
        v.cells[i].occupied = false;
        let removed = memory(&rom, &v).unwrap();
        for y in 0..160 {
            for x in 0..240 {
                let p = 4 * (y * 240 + x);
                let expected = if x / 32 == i % 6 && y / 32 == i / 6 {
                    &board
                } else {
                    &placed
                };
                assert_eq!(removed.rgba[p..p + 4], expected.rgba[p..p + 4]);
            }
        }
        let mut practice = crate::native_garden_second::BouncePractice::new(&rom).unwrap();
        let first = bounce(&rom, &practice.view()).unwrap();
        for _ in 0..120 {
            practice.tick(0).unwrap();
        }
        assert_ne!(first, bounce(&rom, &practice.view()).unwrap());
    }
}
