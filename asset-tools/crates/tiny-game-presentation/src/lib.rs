//! Original native minigame presentation from three exact supplied sources.
//! No native bytes are embedded. Rules/identity/rewards remain in Rust server.
#![forbid(unsafe_code)]
use serde_json::{Value, json};
use sonic_advance_2_assets::{VerifiedRom, gba, graphics};
use std::{
    error::Error,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

pub type Result<T> = std::result::Result<T, Box<dyn Error>>;
pub const ASOE_SHA256: &str = "989acfa4a2522cb78d4dc55cb75232528ebc8b3f5852afdb28410d276339b9d0";
pub const A3VE_ROM_SHA256: &str =
    "5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b";
pub const A3VE_PAYLOAD_SHA256: &str =
    "33dcbab013c9007ad838d8cbff693eef140768ca4110c9617c4055aa75659efb";
pub const A2NE_PAYLOAD_SHA256: &str =
    "84d4df3f397e25885768d212a46287db706857651102b9145595cfda30cee7ca";

#[derive(Clone)]
pub struct Art {
    pub filename: String,
    pub image: graphics::RenderedMap,
    pub source: Value,
}
pub struct Export {
    pub art: Vec<Art>,
    pub resources: Vec<Value>,
    pub decoded_a3ve: Vec<u8>,
}

/// Exact source helper compatibility: the native BIOS LZ callers ignore the
/// signature byte and these original streams use01. Normalize only that byte
/// for the existing strict10 decoder, after the complete source identity gate.
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

pub fn decode_a3ve(rom: &[u8]) -> Result<Vec<u8>> {
    if rom.len() != 0x800000 || gba::sha256(rom) != A3VE_ROM_SHA256 {
        return Err("unsupported A3VE cartridge".into());
    }
    let header = gba::GbaHeader::parse(rom)?.report();
    if header.game_code != "A3VE"
        || header.revision != 0
        || !header.checksum_valid
        || !header.fixed_value_valid
    {
        return Err("A3VE header mismatch".into());
    }
    let result = gba::decode_lz77(&rom[0xd0970..], 0x40000)?;
    if result.consumed != 152552
        || result.bytes.len() != 188716
        || gba::sha256(&result.bytes) != A3VE_PAYLOAD_SHA256
    {
        return Err("A3VE Tiny payload mismatch".into());
    }
    Ok(result.bytes)
}

struct Scene<'a> {
    game: &'static str,
    source: &'a [u8],
    vram: Vec<u8>,
    bg_palette: Vec<u8>,
    obj_palette: Vec<u8>,
    resources: Vec<Value>,
}
impl<'a> Scene<'a> {
    fn new(game: &'static str, source: &'a [u8]) -> Self {
        Self {
            game,
            source,
            vram: vec![0; 0x18000],
            bg_palette: vec![0; 512],
            obj_palette: vec![0; 512],
            resources: Vec::new(),
        }
    }
    fn load(
        &mut self,
        id: &str,
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
        self.resources.push(json!({"game":self.game,"id":id,"offset":offset,"storedHeaderByte":self.source[offset],"compressedLength":decoded.consumed,"compressedSha256":gba::sha256(&self.source[offset..offset+decoded.consumed]),"decodedLength":decoded.bytes.len(),"decodedSha256":gba::sha256(&decoded.bytes),"destinationSpace":space,"destinationOffset":destination}));
        Ok(decoded.bytes)
    }
    fn load_raw(
        &mut self,
        id: &str,
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
        self.resources.push(json!({"game":self.game,"id":id,"offset":offset,"length":length,"sha256":gba::sha256(bytes),"destinationSpace":space,"destinationOffset":destination,"copy":if self.game=="A3VE"{"CpuFastSet32"}else{"CpuSet16"}}));
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
    fn record_layout(&mut self) {
        self.resources.push(json!({"game":self.game,"id":"final-native-layout","bgTilesSha256":gba::sha256(&self.vram[..0x8000]),"bgExtendedTilesSha256":gba::sha256(&self.vram[0x8000..0xc000]),"bgMapsSha256":gba::sha256(&self.vram[0xc000..0xe000]),"objTilesSha256":gba::sha256(&self.vram[0x10000..0x18000]),"bgPaletteSha256":gba::sha256(&self.bg_palette),"objPaletteSha256":gba::sha256(&self.obj_palette)}));
    }
}
fn art(filename: &str, image: graphics::RenderedMap, game: &str, source: Value) -> Art {
    Art {
        filename: filename.into(),
        image,
        source: json!({"game":game,"native":source}),
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
/// A3VE1088 emits each native8-byte descriptor as ordinary 4bpp OAM.
/// Retain its tile offset, shape/size, per-piece flips and signed anchor.
fn descriptor(
    scene: &Scene<'_>,
    offset: usize,
    tile: u16,
    packed: u8,
) -> Result<(graphics::RenderedMap, [i32; 2])> {
    let (image, anchor, _) = descriptor_styled(scene, offset, tile, [0, 0, packed])?;
    Ok((image, anchor))
}
fn descriptor_styled(
    scene: &Scene<'_>,
    offset: usize,
    tile: u16,
    style: [u8; 3],
) -> Result<(graphics::RenderedMap, [i32; 2], Value)> {
    const SIZES: [[(usize, usize); 4]; 3] = [
        [(8, 8), (16, 16), (32, 32), (64, 64)],
        [(16, 8), (32, 8), (32, 16), (64, 32)],
        [(8, 16), (8, 32), (16, 32), (32, 64)],
    ];
    let mut pieces = Vec::new();
    let mut native_parts = Vec::new();
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
        native_parts.push(json!({"tile":base,"attr0High":attr0,"attr1High":attr1,"attr2High":attr2,"anchorOffset":anchor,"textureSize":[width,height],"nativeCanvasSize":if affine&&attr0&2!=0{[width*2,height*2]}else{[width,height]},"affine":affine,"matrixIndex":if affine{Some((attr1>>1)&31)}else{None},"objMode":(attr0>>2)&3}));
        if affine {
            if n != 0 || row[7] != 0 {
                return Err("unmapped multi-piece affine descriptor".into());
            }
            return Ok((
                image,
                anchor,
                json!({"parts":native_parts,"requiresNativeAffine":true,"pixelSpace":"Original untransformed source texture"}),
            ));
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
    Ok((
        output,
        [minx, miny],
        json!({"parts":native_parts,"requiresNativeAffine":false,"pixelSpace":"Original OAM composition before native blend/window effects"}),
    ))
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
    scene.resources.push(json!({"game":scene.game,"id":"bg-template","offset":offset,"length":length,"sha256":gba::sha256(bytes),"destination":dest,"width":width,"height":height,"entryAddend":base,"sourceCallback":"02001718"}));
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

fn memory(source: &[u8]) -> Result<(Vec<Art>, Vec<Value>)> {
    let mut scene = Scene::new("A2NE", source);
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
    let oam = native_lz(source, 0x14d6c)?;
    scene.resources.push(json!({"game":"A2NE","id":"initial-oam","offset":0x14d6c,"decodedSha256":gba::sha256(&oam.bytes),"decodedLength":oam.bytes.len()}));
    let mut output = vec![art(
        "memory-card-back.png",
        scene.sprite(0x6f, 5, 32, 32)?,
        "A2NE",
        json!({"tile":0x6f,"paletteBank":5,"mapping":"1D","sourceCallback":"0200A45C"}),
    )];
    for pair in 0..7 {
        let tile = 0x7f + pair * 16;
        let bank = 2 + (pair >> 1);
        output.push(art(&format!("memory-card-{pair}.png"),scene.sprite(tile as u16,bank as u8,32,32)?,"A2NE",json!({"publicPair":pair,"tile":tile,"paletteBank":bank,"mapping":"1D","sourceCallback":"0200A45C"})));
    }
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
    for (zone, base) in [(1, 0x98), (3, 0xa1), (5, 0xaa)] {
        let entries = template
            .iter()
            .map(|&v| base + u16::from(v))
            .collect::<Vec<_>>();
        output.push(art(
            &format!("memory-zone-{zone}.png"),
            scene.bg(&entries, 4, 4, false)?,
            "A2NE",
            json!({"baseTile":base,"templateOffset":0x15e10,"sourceCallback":"0200A944..0200AA6E"}),
        ));
    }
    for (id, offset, dest) in [
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
        scene.resources.push(json!({"game":"A2NE","id":id,"offset":offset,"decodedSha256":gba::sha256(&decoded.bytes),"rowBytes":12,"rows":20,"destination":dest}));
    }
    let mut bg = backdrop(&scene)?;
    for offset in [0xd800, 0xd000, 0xc800] {
        blit(&mut bg, &scene.screen(offset)?, 0, 0)?;
    }
    output.push(art("memory-board.png",bg,"A2NE",json!({"sourceCallback":"0200A944","layers":["BG3@D800","BG2@D000","BG1@C800"],"size":[240,160],"language":"English"})));
    scene.record_layout();
    Ok((output, scene.resources))
}

fn actor(scene: &Scene<'_>, frame: u16) -> Result<graphics::RenderedMap> {
    let mut out = graphics::RenderedMap {
        width: 24,
        height: 32,
        rgba: vec![0; 24 * 32 * 4],
    };
    // Original ASOE8190/8284/8324 five-object helper, facing0, base(0,8).
    // Lower OAM index wins overlapping opaque pixels, so draw in reverse.
    for (tile, w, h, x, y) in [
        (108, 8, 8, 12, 0),
        (frame + 8, 8, 8, 16, 24),
        (frame + 6, 16, 8, 0, 24),
        (frame + 4, 8, 16, 16, 8),
        (frame, 16, 16, 0, 8),
    ] {
        blit(&mut out, &scene.sprite(tile, 0, w, h)?, x, y)?;
    }
    Ok(out)
}
fn rps(source: &[u8]) -> Result<(Vec<Art>, Vec<Value>)> {
    let mut scene = Scene::new("ASOE", source);
    for (id, offset, space, dest) in [
        ("bg-palette", 0x10e94, "bgPalette", 0),
        ("bg-common", 0xe988, "vram", 0),
        ("bg-rps", 0x10fa4, "vram", 0x1300),
        ("bg-english", 0xf5a8, "vram", 0xf00),
        ("bg-rps-english", 0x112f8, "vram", 0x19a0),
        ("bg-map", 0x11424, "vram", 0xd800),
        ("obj-palette", 0x11578, "objPalette", 0x20),
        ("obj-common", 0xf700, "vram", 0x10000),
        ("obj-rps", 0x115e0, "vram", 0x10de0),
    ] {
        scene.load(id, offset, space, dest)?;
    }
    scene.obj_palette[..32].copy_from_slice(&source[0x1c700..0x1c720]);
    scene.resources.push(json!({"game":"ASOE","id":"normal-actor-palette","offset":0x1c700,"length":32,"sha256":gba::sha256(&source[0x1c700..0x1c720]),"destination":"OBJbank0","sourceCallback":"020019A8"}));
    let mut output = Vec::new();
    for (kind, offset) in [("target", 0x11f78), ("card", 0x11fb4)] {
        for id in 0..3 {
            let row = &source[offset + id * 4..offset + id * 4 + 4];
            let bank = row[0];
            let tile = u16::from(row[1]);
            output.push(art(&format!("rps-{kind}-{id}.png"),scene.sprite(tile,bank,32,32)?,"ASOE",json!({"gesture":id,"tableOffset":offset,"tile":tile,"paletteBank":bank,"mapping":"1D"})));
        }
    }
    output.push(art(
        "rps-card-back.png",
        scene.sprite(0xaf, 3, 32, 32)?,
        "ASOE",
        json!({"tile":0xaf,"paletteBank":3,"sourceCallback":"0200B18A/0200AFB6"}),
    ));
    for frame in [0, 27, 54, 63, 72] {
        output.push(art(&format!("rps-actor-frame-{frame}.png"),actor(&scene,frame)?,"ASOE",json!({"frameTile":frame,"nativeParts":5,"paletteOffset":0x1c700,"appearance":0,"nativeOrigin":[0,8],"facing":0,"sourceCallbacks":["02008284","020082FC","02008324"]})));
    }
    // Exact original static right-panel tile writes in B626..B6CE.
    for (base, tiles) in [
        (0xd0f0, [0x7005_u16, 0x7006, 0x7007, 0x7008]),
        (0xd270, [0x10c9, 0x10ca, 0x10cb, 0x10cc]),
        (0xd430, [0x10c5, 0x10c6, 0x10c7, 0x10c8]),
    ] {
        for (i, &tile) in tiles.iter().enumerate() {
            let at = base + (i / 2) * 64 + (i % 2) * 2;
            scene.vram[at..at + 2].copy_from_slice(&tile.to_le_bytes());
        }
    }
    for (at, tile) in [
        (0xd0f6, 0x2001_u16),
        (0xd136, 0x2002),
        (0xd0f4, 0x2003),
        (0xd134, 0x2004),
        (0xd276, 0x209c),
        (0xd2b6, 0x209d),
        (0xd274, 0x209e),
        (0xd2b4, 0x209f),
        (0xd436, 0x2098),
        (0xd476, 0x2099),
    ] {
        scene.vram[at..at + 2].copy_from_slice(&tile.to_le_bytes());
    }
    let mut bg = backdrop(&scene)?;
    for offset in [0xd800, 0xd000] {
        blit(&mut bg, &scene.screen(offset)?, 0, 0)?;
    }
    output.push(art("rps-background.png",bg,"ASOE",json!({"sourceCallback":"0200B55C","layers":["BG3@D800","BG2@D000"],"size":[240,160],"language":"English","dynamicDigitsIncluded":false})));
    scene.record_layout();
    Ok((output, scene.resources))
}

fn ball(source: &[u8]) -> Result<(Vec<Art>, Vec<Value>)> {
    let mut scene = Scene::new("A3VE", source);
    scene.load_raw("obj-palette", 0x1606c, 0x38 * 4, "objPalette", 0x20)?;
    scene.load_raw("bg-palette", 0x17074, 0x68 * 4, "bgPalette", 0)?;
    for (id, offset, space, dest) in [
        ("obj-tiles", 0x1614c, "vram", 0x10000),
        ("bg-tiles", 0x17214, "vram", 0x40),
    ] {
        scene.load(id, offset, space, dest)?;
    }
    scene.obj_palette[..32].copy_from_slice(&source[0x24050..0x24070]);
    scene.resources.push(json!({"game":"A3VE","id":"normal-actor-palette","offset":0x24050,"length":32,"sha256":gba::sha256(&source[0x24050..0x24070]),"destination":"OBJbank0","sourceCallback":"02001DA4"}));
    let mut output = Vec::new();
    for (name, word) in [
        ("ball-ceiling.png", 0x8205_u16),
        ("ball-color-1.png", 0xa209),
        ("ball-color-2.png", 0x9209),
        ("ball-ring-1.png", 0xb20d),
        ("ball-ring-5.png", 0xb211),
        ("ball-ring-10.png", 0xb215),
    ] {
        output.push(art(
            name,
            scene.bg(&[word, word + 1, word + 2, word + 3], 2, 2, true)?,
            "A3VE",
            json!({"topLeftEntry":word,"fourConsecutiveTiles":true,"sourceCallback":"0200E888"}),
        ));
    }
    // Original D294 sidebar and frame writes;1718 adds native halfwords.
    for (offset, dest, width, height, base) in [
        (0x2dd64, 0xd030, 6, 5, 0x61b6),
        (0x2ddb8, 0xd172, 3, 9, 0x71d6),
        (0x2ddee, 0xd430, 6, 4, 0x61e1),
        // Language flag1 uses the original English two-row text replacement.
        (0x2dda0, 0xd030, 6, 2, 0x61b6),
        (0x2de1e, 0xd800, 2, 20, 1),
        (0x2de6e, 0xd82c, 8, 20, 0x27),
    ] {
        template(&mut scene, offset, dest, width, height, base)?;
    }
    let put = |scene: &mut Scene<'_>, at: usize, word: u16| {
        scene.vram[at..at + 2].copy_from_slice(&word.to_le_bytes());
    };
    for row in 0..20 {
        put(&mut scene, 0xd002 + row * 64, 0x81f8);
        put(&mut scene, 0xd02c + row * 64, 0x81f9);
    }
    for row in 0..16 {
        for col in 0..20 {
            put(
                &mut scene,
                0xd004 + row * 64 + col * 2,
                0x81fa + ((row % 2) * 2 + col % 2) as u16,
            );
        }
    }
    let mut tile = 0x81fe;
    for row in 0..4 {
        put(&mut scene, 0xd404 + row * 64, tile);
        if row != 3 {
            tile += 1;
        }
        for col in 1..20 {
            put(&mut scene, 0xd404 + row * 64 + col * 2, tile);
        }
        tile += 1;
    }
    let mut bg = backdrop(&scene)?;
    // BG3 has the original side decoration, BG2 the playfield border/text.
    blit(
        &mut bg,
        &scene.screen_depth(0xd800, graphics::TileDepth::EightBpp)?,
        0,
        0,
    )?;
    blit(&mut bg, &scene.screen(0xd000)?, 0, 0)?;
    output.push(art("ball-background.png",bg,"A3VE",json!({"sourceCallback":"0200D294","layers":["BG3@D800","BG2@D000"],"size":[240,160],"languageFlag":1,"language":"English","dynamicBoardIncluded":false})));
    for frame in 0..18 {
        for facing in 0..=1 {
            let offset = 0x2dcfc + facing * 16;
            let base = 0x53 + 12 * frame;
            let (image, anchor) = descriptor(&scene, offset, base, 0x0c)?;
            output.push(art(&format!("ball-player-{facing}-{frame}.png"),image,"A3VE",json!({"pose":frame,"facing":facing,"tile":base,"packedPaletteLayer":0x0c,"descriptorOffset":offset,"anchorOffset":anchor,"paletteOffset":0x24050,"appearance":0,"sourceCallbacks":["0200DE24","0200DF8E","02001088"]})));
        }
    }
    for color in 1..=2 {
        let packed = if color == 1 { 0x4c } else { 0x3c };
        for frame in 0..4 {
            let base = 0x12b + frame * 4;
            let (image, offset) = descriptor(&scene, 0x2dd1c, base, packed)?;
            output.push(art(&format!("ball-shot-{color}-{frame}.png"),image,"A3VE",json!({"color":color,"trailFrame":frame,"tile":base,"packedPaletteLayer":packed,"descriptorOffset":0x2dd1c,"anchorOffset":offset,"sourceCallback":"0200E134"})));
        }
    }
    for (name, tile, packed) in [
        ("falling", 0x13f, 0x6c),
        ("caught-0", 0x143, 0x7c),
        ("caught-1", 0x147, 0x7c),
    ] {
        let (image, offset) = descriptor(&scene, 0x2dd5c, tile, packed)?;
        output.push(art(&format!("ball-ring-{name}.png"),image,"A3VE",json!({"tile":tile,"packedPaletteLayer":packed,"descriptorOffset":0x2dd5c,"anchorOffset":offset,"sourceCallbacks":["0200F2DC","0200F194"],"nativeCaughtFlipH":"phase bit1"})));
    }
    scene.record_layout();
    Ok((output, scene.resources))
}

fn bounce(source: &[u8]) -> Result<(Vec<Art>, Vec<Value>)> {
    let mut scene = Scene::new("A2NE-Bounce", source);
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
    scene.resources.push(json!({"game":scene.game,"id":"world-map","offset":0x18624,"compressedLength":decoded.consumed,"compressedSha256":gba::sha256(&source[0x18624..0x18624+decoded.consumed]),"decodedSha256":gba::sha256(&decoded.bytes),"destination":0xd000,"width":24,"height":40,"entryAddend":0x200,"sourceCallback":"0200D360"}));
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
    let mut output = vec![
        art(
            "bounce-world.png",
            world,
            "A2NE-Bounce",
            json!({"sourceCallback":"0200D294","layer":"BG3","mapOffset":0xd000,"depth":8,"nativeBgControl":0x9a83,"scrollRange":[0,160],"languageFlag":1,"language":"English"}),
        ),
        art(
            "bounce-hud.png",
            hud,
            "A2NE-Bounce",
            json!({"sourceCallback":"0200D294","layers":["BG2@C800","BG1@C000","BG0@B800"],"languageFlag":1,"language":"English","dynamicDigitsIncluded":false}),
        ),
    ];
    let poses: Value = serde_json::from_str(include_str!("../catalog/bounce-poses.json"))?;
    for pose in poses
        .as_array()
        .ok_or("Bounce pose metadata must be an array")?
    {
        let offset = usize::from_str_radix(
            pose["descriptor_offset"]
                .as_str()
                .ok_or("missing descriptor")?
                .trim_start_matches("0x"),
            16,
        )?;
        let tile = u16::try_from(pose["tile"].as_u64().ok_or("missing tile")?)?;
        let style = u8::try_from(pose["flags16"].as_u64().ok_or("missing style")?)?;
        let transform = u8::try_from(pose["flags17"].as_u64().ok_or("missing transform")?)?;
        let packed = u8::try_from(
            pose["packed_palette_layer"]
                .as_u64()
                .ok_or("missing palette/layer")?,
        )?;
        let (image, anchor, composition) =
            descriptor_styled(&scene, offset, tile, [style, transform, packed])?;
        output.push(art(&format!("bounce-obj-{offset:x}-{tile}-{style}-{transform}-{packed}.png"),image,"A2NE-Bounce",json!({"descriptorOffset":offset,"tile":tile,"style":style,"transform":transform,"packedPaletteLayer":packed,"anchorOffset":anchor,"composition":composition,"sourceCallback":"02001088","reachability":"Original02001398sprite submissions across48503nativeframes"})));
    }
    scene.record_layout();
    Ok((output, scene.resources))
}

/// Garden body tiles are nine 8x8 characters per pose. The two original OAM
/// descriptors compose each facing; the emotion orb is an independent task.
pub fn garden_art(rom:&[u8])->Result<Vec<Art>>{
 let verified=VerifiedRom::parse(rom)?;let source=verified.garden_bytes();
 if gba::sha256(source)!=A2NE_PAYLOAD_SHA256{return Err("Garden fingerprint mismatch".into());}
 let mut scene=Scene::new("A2NE-Garden",source);
 scene.load_raw("garden-palette",0x246e4,512,"objPalette",0)?;
 scene.load_raw("normal-palette",0x248e4,32,"objPalette",0)?;
 let body=scene.load("garden-body",0x24c24,"vram",0x12d80)?;
 scene.load("garden-items",0x268b4,"vram",0x11580)?;
 let first:u16=0x1a4;
 let count=(body.len()/32-(usize::from(first)-0x2d80/32))/9;
 if count!=67{return Err("Unexpected garden body extent".into());}
 let mut out=Vec::new();
 for (facing,d) in [(0,0x203b4),(1,0x203d4)]{
 let mut sheet=graphics::RenderedMap{width:8*40,height:9*40,rgba:vec![0;8*40*9*40*4]};
 for frame in 0..count{
  let (image,offset)=descriptor(&scene,d,first+9*frame as u16,12)?;
  let xy=source.get(0x20494+frame*5..0x20496+frame*5).ok_or("missing orb anchor")?;
  let (orb,orb_offset)=descriptor(&scene,0x203fc,0x16c,12)?;
  let x=(frame%8*40+20)as i32;let y=(frame/8*40+35)as i32;
  blit(&mut sheet,&image,(x+offset[0]).try_into()?,(y+offset[1]).try_into()?)?;
  let dx=i32::from(xy[0] as i8);
  blit(&mut sheet,&orb,(x+if facing==1{-dx-6}else{dx}+orb_offset[0]).try_into()?,(y+i32::from(xy[1] as i8)+orb_offset[1]).try_into()?)?;
 }
 out.push(art(&format!("garden-normal-{facing}.png"),sheet,"A2NE-Garden",json!({"frames":count,"cell":[40,40],"anchor":[20,35],"descriptor":d,"emotion":"neutral at settled native anchor; no independent bob/alternate expression"})));
 }
 for (id,frame,d,packed) in [(0,0,0x205e4,0x5c),(1,0,0x20604,0x7c),(2,0,0x20614,0x6c),(2,1,0x2060c,0x6c)]{
  let (im,anchor)=descriptor(&scene,d,0xe0,packed)?;
  out.push(art(&format!("garden-toy-{id}-{frame}.png"),im,"A2NE-Garden",json!({"id":id,"frame":frame,"descriptor":d,"anchorOffset":anchor})));
 }
 // Shop task 02007500: original 12-piece panel, anchor (0,80).
 let (panel,anchor,composition)=descriptor_styled(&scene,0x2063c,0x115,[0,0,0xfc])?;
 out.push(art("garden-shop-panel.png",panel,"A2NE-Garden",json!({"descriptor":0x2063c,"tile":0x115,"packed":0xfc,"anchorOffset":anchor,"composition":composition})));
 // 02008c0c and 02007500: decimal digits use tiles 141..14a, OBJ bank 1.
 let mut digits=graphics::RenderedMap{width:80,height:8,rgba:vec![0;80*8*4]};
 for digit in 0..10 {blit(&mut digits,&scene.sprite(0x141+digit,1,8,8)?,usize::from(digit)*8,0)?;}
 out.push(art("garden-shop-digits.png",digits,"A2NE-Garden",json!({"tile":0x141,"paletteBank":1,"count":10,"cell":[8,8],"descriptor":0x2069c})));
 Ok(out)
}
pub fn export_garden(rom:&[u8],output:&Path)->Result<()>{
 let images=garden_art(rom)?;fs::create_dir(output)?;let mut entries=Vec::new();
 for a in images {let png=png_bytes(&a.image)?;write_new(&output.join(&a.filename),&png)?;
 entries.push(json!({"filename":a.filename,"width":a.image.width,"height":a.image.height,"sha256":gba::sha256(&png),"source":a.source}));}
 write_new(&output.join("catalog.json"),&serde_json::to_vec_pretty(&json!({"assets":entries,"sourceSha256":A2NE_PAYLOAD_SHA256}))?)
}

pub fn render(a2ne_rom: &[u8], asoe: &[u8], a3ve_rom: &[u8]) -> Result<Export> {
    let a2ne = VerifiedRom::parse(a2ne_rom)?;
    if gba::sha256(a2ne.garden_bytes()) != A2NE_PAYLOAD_SHA256 {
        return Err("A2NE payload mismatch".into());
    }
    if asoe.len() != 138752 || gba::sha256(asoe) != ASOE_SHA256 {
        return Err("unsupported ASOE payload".into());
    }
    let a3ve = decode_a3ve(a3ve_rom)?;
    let mut art = Vec::new();
    let mut resources = Vec::new();
    for (images, spans) in [
        memory(a2ne.garden_bytes())?,
        rps(asoe)?,
        ball(&a3ve)?,
        bounce(a2ne.garden_bytes())?,
    ] {
        art.extend(images);
        resources.extend(spans);
    }
    Ok(Export {
        art,
        resources,
        decoded_a3ve: a3ve,
    })
}
pub fn png_bytes(image: &graphics::RenderedMap) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder =
            png::Encoder::new(&mut out, image.width.try_into()?, image.height.try_into()?);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header()?.write_image_data(&image.rgba)?;
    }
    Ok(out)
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
pub fn write_export(output: &Path, export: &Export) -> Result<Value> {
    fs::create_dir(output)?;
    let mut entries = Vec::new();
    for asset in &export.art {
        let png = png_bytes(&asset.image)?;
        write_new(&output.join(&asset.filename), &png)?;
        entries.push(json!({"key":asset.filename.trim_end_matches(".png"),"filename":asset.filename,"width":asset.image.width,"height":asset.image.height,"sha256":gba::sha256(&png),"rgbaSha256":gba::sha256(&asset.image.rgba),"source":asset.source}));
    }
    let manifest = json!({"schemaVersion":1,"kind":"original-native-tiny-minigame-art","assets":entries,"sourceResources":export.resources,"sourceIdentities":{"A2NE":A2NE_PAYLOAD_SHA256,"ASOE":ASOE_SHA256,"A3VE":A3VE_PAYLOAD_SHA256},"pixelPolicy":"Original4bpp/8bpp indices, original BGR555 palette bit-replication, transparent index0 only; no filtering/recolor/redraw","limits":["Native gameplay/identity/saves/rewards remain server/core authority","Native OAM affine/priority/scene effects are separate from these decoded static subjects","Chao subjects use native normal appearance0 palette; canonical alternate appearance mapping is separate"]});
    write_new(
        &output.join("catalog.json"),
        &serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_source_01_compatibility_is_normalized() {
        assert_eq!(
            native_lz(&[1, 3, 0, 0, 0, 1, 2, 3], 0).unwrap().bytes,
            [1, 2, 3]
        );
        assert!(native_lz(&[2, 3, 0, 0, 0, 1, 2, 3], 0).is_err());
    }
    #[test]
    fn wrong_whole_sources_fail_before_version_offsets() {
        assert!(render(&[], &[], &[]).is_err());
        assert!(decode_a3ve(&[]).is_err());
    }
    #[test]
    fn obj_span_cannot_carry_into_palette_or_flip_bits() {
        let scene = Scene::new("synthetic", &[]);
        assert!(scene.sprite(1023, 0, 16, 16).is_err());
        assert!(scene.sprite(0, 16, 8, 8).is_err());
        assert!(scene.sprite(0, 0, 0, 8).is_err());
        assert!(scene.sprite(1023, 0, 8, 8).is_ok());
    }
    #[test]
    fn png_roundtrip_keeps_exact_rgba() {
        let image = graphics::RenderedMap {
            width: 2,
            height: 1,
            rgba: vec![3, 4, 5, 0, 248, 16, 8, 255],
        };
        let bytes = png_bytes(&image).unwrap();
        let mut reader = png::Decoder::new(bytes.as_slice()).read_info().unwrap();
        let mut out = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut out).unwrap();
        out.truncate(info.buffer_size());
        assert_eq!(out, image.rgba);
    }
    #[test]
    fn actual_supplied_sources_match_native_art_catalog() {
        let names = [
            "TINY_NATIVE_A2NE_ROM",
            "TINY_NATIVE_ASOE_PAYLOAD",
            "TINY_NATIVE_A3VE_ROM",
        ];
        let paths = names.map(std::env::var_os);
        if paths.iter().all(Option::is_none) {
            return;
        }
        assert!(
            paths.iter().all(Option::is_some),
            "provide all three TINY_NATIVE_* source paths"
        );
        let sources = paths.map(|path| fs::read(path.unwrap()).unwrap());
        let result = render(&sources[0], &sources[1], &sources[2]).unwrap();
        let catalog: Value =
            serde_json::from_str(include_str!("../catalog/native-games.json")).unwrap();
        let assets = catalog["assets"].as_array().unwrap();
        assert_eq!(result.art.len(), assets.len());
        for (actual, expected) in result.art.iter().zip(assets) {
            assert_eq!(actual.filename, expected["filename"].as_str().unwrap());
            assert_eq!(
                gba::sha256(&actual.image.rgba),
                expected["rgbaSha256"].as_str().unwrap()
            );
            assert_eq!(
                gba::sha256(&png_bytes(&actual.image).unwrap()),
                expected["sha256"].as_str().unwrap()
            );
        }
        let mut corrupt = sources[1].clone();
        corrupt[17] ^= 1;
        assert!(render(&sources[0], &corrupt, &sources[2]).is_err());
    }
}
