//! Native ASOE sprite layout and a bounded, non-affine 4bpp presentation slice.
//! No sprite pixels, palettes or source animation scripts are embedded here.
use crate::{binary, graphics, Error, Result};

pub const MAX_FRAME_OBJECTS: usize = 128;
pub const MAX_FRAME_PIXELS: usize = 512 * 512;
pub const MAX_VIEWPORT_PIXELS: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameDimensions {
    pub flip: u8,
    pub oam_index: u8,
    pub object_count: u16,
    pub width: u16,
    pub height: u16,
    pub offset_x: i16,
    pub offset_y: i16,
}

impl FrameDimensions {
    pub fn parse(record: &[u8]) -> Result<Self> {
        if record.len() != 12 {
            return Err(Error(
                "sprite dimension record must be exactly 12 bytes".into(),
            ));
        }
        let value = Self {
            flip: record[0],
            oam_index: record[1],
            object_count: u16::from_le_bytes([record[2], record[3]]),
            width: u16::from_le_bytes([record[4], record[5]]),
            height: u16::from_le_bytes([record[6], record[7]]),
            offset_x: i16::from_le_bytes([record[8], record[9]]),
            offset_y: i16::from_le_bytes([record[10], record[11]]),
        };
        if value.flip > 3
            || value.width == 0
            || value.height == 0
            || value.width > 512
            || value.height > 512
        {
            return Err(Error("unsupported sprite dimensions or flip bits".into()));
        }
        Ok(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OamObject {
    pub raw: [u16; 3],
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    pub tile: usize,
    pub palette_bank: usize,
    pub affine: bool,
    pub flip_x: bool,
    pub flip_y: bool,
}

impl OamObject {
    fn from_record(record: &[u8; 6]) -> Result<Self> {
        let raw = [
            u16::from_le_bytes([record[0], record[1]]),
            u16::from_le_bytes([record[2], record[3]]),
            u16::from_le_bytes([record[4], record[5]]),
        ];
        // A normal affine flag is represented here, then checked against the
        // caller's policy by NativeFrame. Disabled/double-size affine, window,
        // blend, mosaic and 8bpp modes remain unsupported.
        if raw[0] & 0x3e00 != 0 {
            return Err(Error(
                "unsupported affine, object mode, mosaic or 8bpp sprite".into(),
            ));
        }
        let shape = usize::from(raw[0] >> 14);
        let size = usize::from(raw[1] >> 14);
        let (width, height) = match (shape, size) {
            (0, 0) => (8, 8),
            (0, 1) => (16, 16),
            (0, 2) => (32, 32),
            (0, 3) => (64, 64),
            (1, 0) => (16, 8),
            (1, 1) => (32, 8),
            (1, 2) => (32, 16),
            (1, 3) => (64, 32),
            (2, 0) => (8, 16),
            (2, 1) => (8, 32),
            (2, 2) => (16, 32),
            (2, 3) => (32, 64),
            _ => return Err(Error("invalid OAM object shape".into())),
        };
        Ok(Self {
            raw,
            x: usize::from(raw[1] & 0x1ff),
            y: usize::from(raw[0] & 0xff),
            width,
            height,
            tile: usize::from(raw[2] & 0x3ff),
            palette_bank: usize::from(raw[2] >> 12),
            affine: raw[0] & 0x100 != 0,
            flip_x: raw[0] & 0x100 == 0 && raw[1] & 0x1000 != 0,
            flip_y: raw[0] & 0x100 == 0 && raw[1] & 0x2000 != 0,
        })
    }
}

/// A borrowed, validated dimension record and selected native six-byte OAM
/// records. The OAM table must begin at this animation's OAM root.
#[derive(Debug)]
pub struct NativeFrame<'a> {
    dimensions: FrameDimensions,
    objects: &'a [[u8; 6]],
}

impl<'a> NativeFrame<'a> {
    pub fn parse(record: &[u8], oam_table: &'a [u8], max_objects: usize) -> Result<Self> {
        let dimensions = FrameDimensions::parse(record)?;
        let count = usize::from(dimensions.object_count);
        if count == 0 || count > max_objects.min(MAX_FRAME_OBJECTS) {
            return Err(Error(
                "sprite object count exceeds budget or is zero".into(),
            ));
        }
        let start = usize::from(dimensions.oam_index) * 6;
        let selected = binary::range(oam_table, start, count * 6)?;
        let objects = selected.as_chunks::<6>().0;
        for record in objects {
            let object = OamObject::from_record(record)?;
            if object.affine {
                return Err(Error("unsupported affine title object".into()));
            }
            if object.x + object.width > usize::from(dimensions.width)
                || object.y + object.height > usize::from(dimensions.height)
            {
                return Err(Error(
                    "object extends beyond the supported sprite rectangle".into(),
                ));
            }
        }
        Ok(Self {
            dimensions,
            objects,
        })
    }

    pub fn objects(&self) -> impl DoubleEndedIterator<Item = OamObject> + '_ {
        // NativeFrame::parse validated the same immutable borrowed bytes.
        self.objects
            .iter()
            .map(|record| OamObject::from_record(record).expect("validated immutable OAM record"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedFrame {
    pub image: graphics::RgbaImage,
    pub offset_x: i16,
    pub offset_y: i16,
}

/// Composite native 1D-mapped object tiles into the frame rectangle. Lower OAM
/// indices cover higher indices; palette index zero is transparent. Frame and
/// per-object flip bits are both preserved. This does not emulate BG priority,
/// nonidentity affine transforms, blending, windows, or wrapped coordinates.
/// Identity-affine objects require the explicit NativeFrame parser above.
pub fn render_frame(
    frame: &NativeFrame<'_>,
    tiles: &[u8],
    palette: &[[u8; 4]],
    max_pixels: usize,
) -> Result<RenderedFrame> {
    if tiles.is_empty() || !tiles.len().is_multiple_of(32) || tiles.len() > 1024 * 32 {
        return Err(Error("invalid or oversized native 4bpp tile load".into()));
    }
    if palette.is_empty() || palette.len() > 256 || palette.iter().any(|color| color[3] != 255) {
        return Err(Error(
            "native palette must contain opaque RGB colors".into(),
        ));
    }
    let width = usize::from(frame.dimensions.width);
    let height = usize::from(frame.dimensions.height);
    let pixels = width
        .checked_mul(height)
        .ok_or_else(|| Error("sprite size overflow".into()))?;
    if pixels > max_pixels.min(MAX_FRAME_PIXELS) {
        return Err(Error("sprite pixel budget exceeded".into()));
    }
    for object in frame.objects() {
        let tile_count = (object.width / 8) * (object.height / 8);
        if object.tile + tile_count > tiles.len() / 32
            || (object.palette_bank + 1) * 16 > palette.len()
        {
            return Err(Error(
                "sprite tile or palette reference is unavailable".into(),
            ));
        }
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(pixels)
        .map_err(|_| Error("sprite allocation failed".into()))?;
    output.resize(pixels, [0; 4]);
    for object in frame.objects().rev() {
        for y in 0..object.height {
            for x in 0..object.width {
                let sx = if object.flip_x {
                    object.width - 1 - x
                } else {
                    x
                };
                let sy = if object.flip_y {
                    object.height - 1 - y
                } else {
                    y
                };
                let tile = object.tile + (sy / 8) * (object.width / 8) + sx / 8;
                let packed = tiles[tile * 32 + (sy % 8) * 4 + (sx % 8) / 2];
                let index = usize::from(if sx.is_multiple_of(2) {
                    packed & 15
                } else {
                    packed >> 4
                });
                if index == 0 {
                    continue;
                }
                let dx = object.x + x;
                let dy = object.y + y;
                let dx = if frame.dimensions.flip & 1 != 0 {
                    width - 1 - dx
                } else {
                    dx
                };
                let dy = if frame.dimensions.flip & 2 != 0 {
                    height - 1 - dy
                } else {
                    dy
                };
                output[dy * width + dx] = palette[object.palette_bank * 16 + index];
            }
        }
    }
    Ok(RenderedFrame {
        image: graphics::RgbaImage {
            width,
            height,
            pixels: output,
        },
        offset_x: frame.dimensions.offset_x,
        offset_y: frame.dimensions.offset_y,
    })
}

/// Overlay a native frame at its anchor into an RGBA byte viewport. Every
/// destination coordinate is clipped using wide signed arithmetic. A global
/// horizontal flip reflects both the image and its anchor, as native layout does.
pub fn composite_rgba(
    target: &mut [u8],
    width: usize,
    height: usize,
    frame: &RenderedFrame,
    anchor_x: i32,
    anchor_y: i32,
    flip_x: bool,
) -> Result<()> {
    let count = width
        .checked_mul(height)
        .ok_or_else(|| Error("viewport dimensions overflow".into()))?;
    let bytes = count
        .checked_mul(4)
        .ok_or_else(|| Error("viewport bytes overflow".into()))?;
    if width == 0 || height == 0 || count > MAX_VIEWPORT_PIXELS || bytes != target.len() {
        return Err(Error("invalid or oversized viewport".into()));
    }
    let image = &frame.image;
    let source_count = image
        .width
        .checked_mul(image.height)
        .ok_or_else(|| Error("frame size overflow".into()))?;
    if image.width == 0
        || image.height == 0
        || source_count > MAX_FRAME_PIXELS
        || source_count != image.pixels.len()
        || image
            .pixels
            .iter()
            .any(|pixel| pixel[3] != 0 && pixel[3] != 255)
    {
        return Err(Error("malformed native frame or nonbinary alpha".into()));
    }
    let offset_x = if flip_x {
        image.width as i64 - i64::from(frame.offset_x)
    } else {
        i64::from(frame.offset_x)
    };
    let left = i64::from(anchor_x) - offset_x;
    let top = i64::from(anchor_y) - i64::from(frame.offset_y);
    for y in 0..image.height {
        let dy = top + y as i64;
        if dy < 0 || dy >= height as i64 {
            continue;
        }
        for x in 0..image.width {
            let dx = left + x as i64;
            if dx < 0 || dx >= width as i64 {
                continue;
            }
            let sx = if flip_x { image.width - 1 - x } else { x };
            let pixel = image.pixels[y * image.width + sx];
            if pixel[3] != 0 {
                let at = (dy as usize * width + dx as usize) * 4;
                target[at..at + 4].copy_from_slice(&pixel);
            }
        }
    }
    Ok(())
}
