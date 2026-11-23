//! Native pixels without tinting, rescaling, premultiplication or alpha removal.
//! GVR/GVP layout: X-Hax/sa_tools TextureLib; DDS layout: Microsoft DDS docs.
use crate::{Error, Result};
use serde::Serialize;
use std::io::Write;

/// GXGetTexBufferSize for supported GX storage formats and valid SDK dimensions.
/// max_lod counts levels (including the base); it is ignored without mipmaps.
pub fn gx_buffer_size(
    width: u16,
    height: u16,
    format: u32,
    mipmaps: bool,
    max_lod: u8,
) -> Result<u32> {
    let (mut w, mut h) = (u32::from(width), u32::from(height));
    if !(1..=1024).contains(&w)
        || !(1..=1024).contains(&h)
        || (mipmaps && !(w.is_power_of_two() && h.is_power_of_two()))
    {
        return Err(Error::invalid("invalid GX texture dimensions"));
    }
    let (tw, th, bytes) = match format {
        0 | 8 | 14 | 0x20 | 0x30 => (8, 8, 32),
        1 | 2 | 9 | 0x11 | 0x22 | 0x27..=0x2a | 0x39 | 0x3a => (8, 4, 32),
        3..=5 | 10 | 0x13 | 0x23 | 0x2b | 0x2c | 0x3c => (4, 4, 32),
        6 | 0x16 => (4, 4, 64),
        _ => return Err(Error::invalid("unsupported GX texture buffer format")),
    };
    let mut size = 0;
    for _ in 0..if mipmaps { max_lod } else { 1 } {
        size += w.div_ceil(tw) * h.div_ceil(th) * bytes;
        if w == 1 && h == 1 {
            break;
        }
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    Ok(size)
}

#[cfg(test)]
mod gx_size_tests {
    use super::*;
    #[test]
    fn partial_tiles_and_mip_tail() {
        assert_eq!(gx_buffer_size(64, 64, 2, false, 0).unwrap(), 4096);
        assert_eq!(gx_buffer_size(9, 5, 2, false, 30).unwrap(), 128);
        assert_eq!(gx_buffer_size(8, 2, 6, true, 1).unwrap(), 128);
        assert_eq!(gx_buffer_size(8, 2, 6, true, 255).unwrap(), 320);
        assert_eq!(gx_buffer_size(8, 2, 6, true, 0).unwrap(), 0);
        assert!(gx_buffer_size(9, 5, 2, true, 1).is_err());
        assert!(gx_buffer_size(4, 4, 7, false, 1).is_err());
    }
}

fn span(data: &[u8], at: usize, len: usize) -> Result<&[u8]> {
    data.get(
        at..at
            .checked_add(len)
            .ok_or_else(|| Error::invalid("texture range overflow"))?,
    )
    .ok_or_else(|| Error::invalid("truncated texture"))
}
fn be16(d: &[u8], a: usize) -> Result<u16> {
    let b = span(d, a, 2)?;
    Ok(u16::from_be_bytes([b[0], b[1]]))
}
fn le32(d: &[u8], a: usize) -> Result<u32> {
    let b = span(d, a, 4)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}
fn dimensions(w: usize, h: usize) -> Result<usize> {
    if w == 0 || h == 0 || w > 4096 || h > 4096 {
        return Err(Error::invalid("texture dimension outside 1..4096"));
    }
    Ok(w * h)
}
fn c5(x: u16) -> u8 {
    ((x << 3) | (x >> 2)) as u8
}
fn c6(x: u16) -> u8 {
    ((x << 2) | (x >> 4)) as u8
}
pub(crate) fn rgb565(v: u16) -> [u8; 4] {
    [c5(v >> 11), c6((v >> 5) & 63), c5(v & 31), 255]
}
fn rgb5a3(v: u16) -> [u8; 4] {
    if v & 0x8000 != 0 {
        [c5((v >> 10) & 31), c5((v >> 5) & 31), c5(v & 31), 255]
    } else {
        let a = ((v >> 12) & 7) as u8;
        [
            (((v >> 8) & 15) * 17) as u8,
            (((v >> 4) & 15) * 17) as u8,
            ((v & 15) * 17) as u8,
            (a << 5) | (a << 2) | (a >> 1),
        ]
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Image {
    pub width: u32,
    pub height: u32,
    #[serde(skip)]
    pub rgba: Vec<u8>,
}
impl Image {
    pub fn write_png(&self, output: impl Write) -> Result<()> {
        if self.rgba.len() != dimensions(self.width as usize, self.height as usize)? * 4 {
            return Err(Error::invalid("RGBA size mismatch"));
        }
        let mut encoder = png::Encoder::new(output, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| Error::invalid(e.to_string()))?;
        writer
            .write_image_data(&self.rgba)
            .map_err(|e| Error::invalid(e.to_string()))?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Palette {
    pub format: u16,
    pub start_bank: u16,
    pub start_color: u16,
    pub colors: Vec<[u8; 4]>,
}
impl Palette {
    /// Bank is relative to this file's first color. Header startBank/startColor
    /// remain metadata; callers select the scene's explicit palette mapping.
    pub fn bank(&self, bank: usize, entries: usize) -> Result<&[[u8; 4]]> {
        if entries != 16 && entries != 256 {
            return Err(Error::invalid("palette bank size must be 16 or 256"));
        }
        let start = bank
            .checked_mul(entries)
            .ok_or_else(|| Error::invalid("palette bank overflow"))?;
        self.colors
            .get(
                start
                    ..start
                        .checked_add(entries)
                        .ok_or_else(|| Error::invalid("palette bank overflow"))?,
            )
            .ok_or_else(|| Error::invalid("palette bank outside file"))
    }
}
fn palette_color(format: u16, b: &[u8]) -> Result<[u8; 4]> {
    let v = if b.len() >= 2 {
        u16::from_be_bytes([b[0], b[1]])
    } else {
        return Err(Error::invalid("short palette color"));
    };
    Ok(match format {
        0 => [
            c5((v >> 10) & 31),
            c5((v >> 5) & 31),
            c5(v & 31),
            if v & 0x8000 != 0 { 255 } else { 0 },
        ],
        1 => rgb565(v),
        2 => [
            (((v >> 8) & 15) * 17) as u8,
            (((v >> 4) & 15) * 17) as u8,
            ((v & 15) * 17) as u8,
            (((v >> 12) & 15) * 17) as u8,
        ],
        6 => {
            let b = span(b, 0, 4)?;
            [b[1], b[2], b[3], b[0]]
        }
        _ => return Err(Error::invalid("unsupported SA2 palette format")),
    })
}
pub fn read_gvp(data: &[u8]) -> Result<Palette> {
    if span(data, 0, 4)? != b"GVPL" {
        return Err(Error::invalid("expected GVPL"));
    }
    let end = (le32(data, 4)? as usize)
        .checked_add(8)
        .ok_or_else(|| Error::invalid("GVP size overflow"))?;
    span(data, 0, end)?;
    let format = be16(data, 8)?;
    let count = be16(data, 14)? as usize;
    let bpp = if format == 6 { 4 } else { 2 };
    if count == 0 || 16 + count * bpp > end {
        return Err(Error::invalid("GVP color count exceeds chunk"));
    }
    let mut colors = Vec::with_capacity(count);
    for i in 0..count {
        colors.push(palette_color(format, span(data, 16 + i * bpp, bpp)?)?);
    }
    Ok(Palette {
        format,
        start_bank: be16(data, 10)?,
        start_color: be16(data, 12)?,
        colors,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GvrHeader {
    pub width: u32,
    pub height: u32,
    pub data_format: u8,
    pub palette_format: u8,
    pub flags: u8,
    pub global_index: Option<u32>,
    pub data_offset: usize,
    pub chunk_end: usize,
}
pub fn read_gvr_header(data: &[u8]) -> Result<GvrHeader> {
    let mut at = 0;
    let mut global_index = None;
    if [b"GBIX".as_slice(), b"GCIX".as_slice()].contains(&span(data, 0, 4)?) {
        let size = le32(data, 4)? as usize;
        if size < 4 {
            return Err(Error::invalid("short GVR global index"));
        }
        span(data, 8, size)?;
        let b = span(data, 8, 4)?;
        global_index = Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
        at = 8 + size;
    }
    if span(data, at, 4)? != b"GVRT" {
        return Err(Error::invalid("expected GVRT"));
    }
    let size = le32(data, at + 4)? as usize;
    if size < 8 {
        return Err(Error::invalid("short GVRT chunk"));
    }
    span(data, at, 8 + size)?;
    let width = be16(data, at + 12)? as u32;
    let height = be16(data, at + 14)? as u32;
    dimensions(width as usize, height as usize)?;
    if !width.is_power_of_two() || !height.is_power_of_two() {
        return Err(Error::invalid("non-power-of-two GVR unsupported"));
    }
    Ok(GvrHeader {
        width,
        height,
        data_format: data[at + 11],
        palette_format: data[at + 10] >> 4,
        flags: data[at + 10] & 15,
        global_index,
        data_offset: at + 16,
        chunk_end: at + 8 + size,
    })
}

/// Indexed textures require an explicit external palette. No grayscale fallback
/// is presented as a decoded original color texture.
pub fn decode_gvr(data: &[u8], external_palette: Option<&[[u8; 4]]>) -> Result<Image> {
    let header = read_gvr_header(data)?;
    let w = header.width as usize;
    let h = header.height as usize;
    let f = header.data_format;
    let mut at = header.data_offset;
    let entries = match f {
        8 => 16,
        9 => 256,
        _ => 0,
    };
    let internal;
    let palette = if entries > 0 && header.flags & 8 != 0 {
        let bpp = if header.palette_format == 6 { 4 } else { 2 };
        let raw = span(&data[..header.chunk_end], at, entries * bpp)?;
        at += entries * bpp;
        internal = (0..entries)
            .map(|i| palette_color(header.palette_format as u16, &raw[i * bpp..(i + 1) * bpp]))
            .collect::<Result<Vec<_>>>()?;
        Some(internal.as_slice())
    } else {
        external_palette
    };
    if entries > 0 && palette.is_none_or(|p| p.len() < entries) {
        return Err(Error::invalid("indexed GVR requires explicit palette bank"));
    }
    let raw = &data[at..header.chunk_end];
    let (bw, bh, bits) = match f {
        0 | 8 => (8.min(w), 8.min(h), 4),
        1 | 2 | 9 => (8.min(w), 4.min(h), 8),
        3..=5 => (4.min(w), 4.min(h), 16),
        6 => (4.min(w), 4.min(h), 32),
        14 => (8.min(w), 8.min(h), 4),
        _ => {
            return Err(Error::invalid(format!(
                "unsupported GVR data format {f:#x}"
            )))
        }
    };
    // Small rectangular split-plane layouts are not established by the local
    // reference corpus. Reject them rather than guessing per-tile padding.
    if f == 6 && (w < 4 || h < 4) && (w > 4 || h > 4) {
        return Err(Error::invalid(
            "small rectangular RGBA GVR layout unsupported",
        ));
    }
    if matches!(f, 0 | 8) && w == 1 {
        return Err(Error::invalid("single-column 4-bit GVR layout unsupported"));
    }
    if f == 14 && (w < 4 || h < 4) {
        return Err(Error::invalid("sub-4-pixel CMPR GVR layout unsupported"));
    }
    let required = if f == 6 {
        (w.div_ceil(4) * h.div_ceil(4) * 64).max(64)
    } else if f == 14 {
        w.div_ceil(4) * h.div_ceil(4) * 8
    } else {
        (w * h * bits / 8).max(32)
    };
    span(raw, 0, required)?;
    let mut rgba = vec![0; w * h * 4];
    if f == 14 {
        decode_cmpr(raw, w, h, &mut rgba)?;
    } else {
        let mut cursor = 0;
        for ty in (0..h).step_by(bh) {
            for tx in (0..w).step_by(bw) {
                for y in 0..bh {
                    for x in 0..bw {
                        let pixel = y * bw + x;
                        let p = match f {
                            0 | 8 => {
                                let idx = (raw[cursor + pixel / 2]
                                    >> if pixel % 2 == 0 { 4 } else { 0 })
                                    & 15;
                                if f == 8 {
                                    palette.ok_or_else(|| Error::invalid("missing palette"))?
                                        [idx as usize]
                                } else {
                                    [idx * 17, idx * 17, idx * 17, 255]
                                }
                            }
                            1 => {
                                let v = raw[cursor + pixel];
                                [v, v, v, 255]
                            }
                            2 => {
                                let v = raw[cursor + pixel];
                                let c = (v & 15) * 17;
                                [c, c, c, (v >> 4) * 17]
                            }
                            3 => {
                                let a = raw[cursor + pixel * 2];
                                let c = raw[cursor + pixel * 2 + 1];
                                [c, c, c, a]
                            }
                            4 | 5 => {
                                let i = cursor + pixel * 2;
                                let v = u16::from_be_bytes([raw[i], raw[i + 1]]);
                                if f == 4 {
                                    rgb565(v)
                                } else {
                                    rgb5a3(v)
                                }
                            }
                            6 => {
                                let i = cursor + pixel * 2;
                                [raw[i + 1], raw[i + 32], raw[i + 33], raw[i]]
                            }
                            9 => palette.ok_or_else(|| Error::invalid("missing palette"))?
                                [raw[cursor + pixel] as usize],
                            _ => return Err(Error::invalid("unsupported GVR")),
                        };
                        let dst = ((ty + y) * w + tx + x) * 4;
                        rgba[dst..dst + 4].copy_from_slice(&p);
                    }
                }
                cursor += if f == 6 { 64 } else { bw * bh * bits / 8 };
            }
        }
    }
    Ok(Image {
        width: header.width,
        height: header.height,
        rgba,
    })
}

fn block_colors(v0: u16, v1: u16, force_four: bool) -> [[u8; 4]; 4] {
    let mut c = [rgb565(v0), rgb565(v1), [0, 0, 0, 255], [0, 0, 0, 255]];
    for channel in 0..3 {
        if v0 > v1 || force_four {
            c[2][channel] = ((2 * c[0][channel] as u16 + c[1][channel] as u16) / 3) as u8;
            c[3][channel] = ((c[0][channel] as u16 + 2 * c[1][channel] as u16) / 3) as u8;
        } else {
            c[2][channel] = ((c[0][channel] as u16 + c[1][channel] as u16) / 2) as u8;
        }
    }
    if v0 <= v1 && !force_four {
        c[3] = [0; 4];
    }
    c
}
fn decode_cmpr(raw: &[u8], w: usize, h: usize, out: &mut [u8]) -> Result<()> {
    let bw = 8.min(w);
    let bh = 8.min(h);
    let tile = 4.min(w).min(h);
    let mut at = 0;
    for ty in (0..h).step_by(bh) {
        for tx in (0..w).step_by(bw) {
            for sy in (0..bh).step_by(tile) {
                for sx in (0..bw).step_by(tile) {
                    let b = span(raw, at, 8)?;
                    at += 8;
                    let c = block_colors(
                        u16::from_be_bytes([b[0], b[1]]),
                        u16::from_be_bytes([b[2], b[3]]),
                        false,
                    );
                    for y in 0..tile {
                        for x in 0..tile {
                            let p = c[((b[4 + y] >> (6 - x * 2)) & 3) as usize];
                            let dst = ((ty + sy + y) * w + tx + sx + x) * 4;
                            out[dst..dst + 4].copy_from_slice(&p);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Base mip of legacy DDS DXT1/3/5 or 32-bit channels. Unsupported DX10 and
/// noncontiguous masks fail explicitly; source alpha is copied unchanged.
pub fn decode_dds(data: &[u8]) -> Result<Image> {
    decode_dds_mip(data, 0)
}

/// Select a declared mip in a legacy 2D DDS texture.
pub fn decode_dds_mip(data: &[u8], level: u32) -> Result<Image> {
    if span(data, 0, 4)? != b"DDS " || le32(data, 4)? != 124 || le32(data, 76)? != 32 {
        return Err(Error::invalid("invalid DDS header"));
    }
    span(data, 0, 128)?;
    let mut w = le32(data, 16)? as usize;
    let mut h = le32(data, 12)? as usize;
    dimensions(w, h)?;
    let mut offset = 128usize;
    let flags = le32(data, 80)?;
    if level > 0 {
        let levels = le32(data, 28)?;
        if le32(data, 8)? & 0x20000 == 0
            || level >= levels
            || levels > w.max(h).ilog2() + 1
            || le32(data, 112)? != 0
            || le32(data, 24)? > 1
        {
            return Err(Error::invalid(
                "invalid mip level or unsupported DDS surface",
            ));
        }
        let block_size = if flags & 4 != 0 {
            match span(data, 84, 4)? {
                b"DXT1" => Some(8),
                b"DXT3" | b"DXT5" => Some(16),
                _ => return Err(Error::invalid("unsupported DDS FourCC")),
            }
        } else {
            if flags & 0x40 == 0
                || le32(data, 88)? != 32
                || (le32(data, 8)? & 8 != 0 && le32(data, 20)? as usize != w * 4)
            {
                return Err(Error::invalid("unsupported mip layout or padded DDS rows"));
            }
            None
        };
        for _ in 0..level {
            let size = match block_size {
                Some(size) => w.div_ceil(4) * h.div_ceil(4) * size,
                None => w * h * 4,
            };
            span(data, offset, size)?;
            offset += size;
            w = (w / 2).max(1);
            h = (h / 2).max(1);
        }
    }
    let count = dimensions(w, h)?;
    let mut rgba;
    if flags & 4 != 0 {
        let fourcc = span(data, 84, 4)?;
        let size = match fourcc {
            b"DXT1" => 8,
            b"DXT3" | b"DXT5" => 16,
            _ => return Err(Error::invalid("unsupported DDS FourCC")),
        };
        span(data, offset, w.div_ceil(4) * h.div_ceil(4) * size)?;
        rgba = vec![0; count * 4];
        let mut at = offset;
        for by in (0..h).step_by(4) {
            for bx in (0..w).step_by(4) {
                let block = span(data, at, size)?;
                at += size;
                let color = if size == 8 { block } else { &block[8..] };
                let c = block_colors(
                    u16::from_le_bytes([color[0], color[1]]),
                    u16::from_le_bytes([color[2], color[3]]),
                    size == 16,
                );
                let codes = u32::from_le_bytes([color[4], color[5], color[6], color[7]]);
                let mut alpha = [255; 16];
                if fourcc == b"DXT3" {
                    for i in 0..16 {
                        alpha[i] = ((block[i / 2] >> ((i % 2) * 4)) & 15) * 17;
                    }
                }
                if fourcc == b"DXT5" {
                    let mut a = [0u8; 8];
                    a[0] = block[0];
                    a[1] = block[1];
                    if a[0] > a[1] {
                        for i in 1..7 {
                            a[i + 1] = (((7 - i) * a[0] as usize + i * a[1] as usize) / 7) as u8;
                        }
                    } else {
                        for i in 1..5 {
                            a[i + 1] = (((5 - i) * a[0] as usize + i * a[1] as usize) / 5) as u8;
                        }
                        a[6] = 0;
                        a[7] = 255;
                    }
                    let mut codes = 0u64;
                    for i in 0..6 {
                        codes |= (block[2 + i] as u64) << (i * 8);
                    }
                    for (i, v) in alpha.iter_mut().enumerate() {
                        *v = a[((codes >> (i * 3)) & 7) as usize];
                    }
                }
                for y in 0..4 {
                    for x in 0..4 {
                        if bx + x < w && by + y < h {
                            let i = y * 4 + x;
                            let mut p = c[((codes >> (i * 2)) & 3) as usize];
                            if size == 16 {
                                p[3] = alpha[i];
                            }
                            let dst = ((by + y) * w + bx + x) * 4;
                            rgba[dst..dst + 4].copy_from_slice(&p);
                        }
                    }
                }
            }
        }
    } else {
        if flags & 0x40 == 0 || le32(data, 88)? != 32 {
            return Err(Error::invalid("unsupported DDS pixel layout"));
        }
        let masks = [
            le32(data, 92)?,
            le32(data, 96)?,
            le32(data, 100)?,
            le32(data, 104)?,
        ];
        if masks[..3].contains(&0)
            || masks
                .iter()
                .enumerate()
                .any(|(i, m)| masks.iter().skip(i + 1).any(|n| m & n != 0))
        {
            return Err(Error::invalid("invalid DDS masks"));
        }
        for m in masks {
            if m != 0 && m >> m.trailing_zeros() != 255 {
                return Err(Error::invalid("DDS requires 8-bit contiguous masks"));
            }
        }
        let pitch = if level == 0 && le32(data, 8)? & 8 != 0 {
            le32(data, 20)? as usize
        } else {
            w * 4
        };
        if pitch < w * 4 {
            return Err(Error::invalid("short DDS row pitch"));
        }
        span(
            data,
            offset,
            pitch
                .checked_mul(h)
                .ok_or_else(|| Error::invalid("DDS pitch overflow"))?,
        )?;
        rgba = vec![0; count * 4];
        for y in 0..h {
            for x in 0..w {
                let v = le32(data, offset + y * pitch + x * 4)?;
                for (i, m) in masks.iter().enumerate() {
                    rgba[(y * w + x) * 4 + i] = if *m == 0 {
                        255
                    } else {
                        ((v & m) >> m.trailing_zeros()) as u8
                    };
                }
            }
        }
    }
    Ok(Image {
        width: w as u32,
        height: h as u32,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn gvr(f: u8, flags: u8, w: u16, h: u16, pixels: &[u8]) -> Vec<u8> {
        let mut b = b"GVRT".to_vec();
        b.extend(((pixels.len() + 8) as u32).to_le_bytes());
        b.extend([0, 0, flags, f]);
        b.extend(w.to_be_bytes());
        b.extend(h.to_be_bytes());
        b.extend(pixels);
        b
    }
    #[test]
    fn rgba_planes_preserve_original_alpha() {
        let mut raw = vec![0; 64];
        raw[0] = 17;
        raw[1] = 22;
        raw[32] = 33;
        raw[33] = 44;
        let image = decode_gvr(&gvr(6, 0, 4, 4, &raw), None).unwrap();
        assert_eq!(&image.rgba[..4], &[22, 33, 44, 17]);
    }
    #[test]
    fn external_palette_mandatory_and_high_nibble_first() {
        let raw = vec![0x12; 32];
        let data = gvr(8, 2, 8, 8, &raw);
        assert!(decode_gvr(&data, None).is_err());
        let mut palette = [[0; 4]; 16];
        palette[1] = [1, 2, 3, 4];
        palette[2] = [5, 6, 7, 8];
        let image = decode_gvr(&data, Some(&palette)).unwrap();
        assert_eq!(&image.rgba[..8], &[1, 2, 3, 4, 5, 6, 7, 8]);
    }
    #[test]
    fn rgb5a3_and_cmpr_alpha() {
        assert_eq!(rgb5a3(0x0123), [17, 34, 51, 0]);
        assert_eq!(rgb5a3(0xffff), [255; 4]);
        let b = gvr(14, 0, 4, 4, &[0, 0, 255, 255, 255, 255, 255, 255]);
        assert!(decode_gvr(&b, None)
            .unwrap()
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [0; 4]));
    }
    #[test]
    fn gvp_banks_and_truncation() {
        let mut b = b"GVPL".to_vec();
        b.extend(104u32.to_le_bytes());
        b.extend([0, 1, 0, 0, 0, 0, 0, 48]);
        b.extend(vec![255; 96]);
        let p = read_gvp(&b).unwrap();
        assert_eq!(p.bank(2, 16).unwrap().len(), 16);
        assert!(p.bank(3, 16).is_err());
        b.pop();
        assert!(read_gvp(&b).is_err());
    }
    #[test]
    fn corrupt_textures_fail_without_panicking() {
        for n in 0..128 {
            let d = vec![0; n];
            assert!(decode_dds(&d).is_err());
            assert!(decode_gvr(&d, None).is_err());
            assert!(read_gvp(&d).is_err());
        }
    }
    #[test]
    fn rgba_gvr_split_planes_preserve_alpha() {
        let mut b = b"GVRT".to_vec();
        b.extend(72u32.to_le_bytes());
        b.extend([0, 0, 0, 6, 0, 4, 0, 4]);
        for i in 0..16 {
            b.extend([i, 200]);
        }
        for _ in 0..16 {
            b.extend([30, 40]);
        }
        let image = decode_gvr(&b, None).unwrap();
        for (i, p) in image.rgba.as_chunks::<4>().0.iter().enumerate() {
            assert_eq!(*p, [200, 30, 40, i as u8]);
        }
    }
    fn dds_header(code: &[u8; 4]) -> Vec<u8> {
        let mut b = vec![0; 128];
        b[..4].copy_from_slice(b"DDS ");
        b[4..8].copy_from_slice(&124u32.to_le_bytes());
        b[12..16].copy_from_slice(&4u32.to_le_bytes());
        b[16..20].copy_from_slice(&4u32.to_le_bytes());
        b[76..80].copy_from_slice(&32u32.to_le_bytes());
        b[80..84].copy_from_slice(&4u32.to_le_bytes());
        b[84..88].copy_from_slice(code);
        b
    }
    #[test]
    fn dds_dxt5_alpha_endpoints_and_interpolation() {
        let mut b = dds_header(b"DXT5");
        let mut a = [0; 16];
        a[0] = 0;
        a[1] = 100;
        // Alpha index 7 is opaque; index 6 is transparent in the a0<=a1 mode.
        a[2] = 0b00110111;
        a[8..10].copy_from_slice(&0xf800u16.to_le_bytes());
        a[10..12].copy_from_slice(&0x07e0u16.to_le_bytes());
        b.extend(a);
        let image = decode_dds(&b).unwrap();
        assert_eq!(&image.rgba[..8], &[255, 0, 0, 255, 255, 0, 0, 0]);
        b.pop();
        assert!(decode_dds(&b).is_err());
    }
    #[test]
    fn dds_unknown_codec_does_not_synthesize_pixels() {
        assert!(decode_dds(&dds_header(b"DX10")).is_err());
    }
}
