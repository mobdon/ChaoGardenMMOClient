//! Stage textures: a PRS-packed GVM archive of GVR textures (Iron Gate's
//! `landtx11.prs` on Steam), in archive order (the order of the stage's
//! texture list). GVR texel formats are GX's, so sa2-gx decodes them; the
//! palette formats go through the sa2 crate's decoder.
use crate::gpu::Rgba;
use sa2_asset_tools::{archive, texture};
use sa2_gx::texture::{decode_mip_chain, TextureInput};

fn decode(data: &[u8]) -> Option<Rgba> {
    let h = texture::read_gvr_header(data).ok()?;
    if matches!(h.data_format, 8 | 9) {
        let i = texture::decode_gvr(data, None).ok()?;
        return Some(Rgba {
            width: i.width,
            height: i.height,
            pixels: i.rgba,
        });
    }
    let input = TextureInput {
        format: u32::from(h.data_format),
        width: h.width,
        height: h.height,
        data: data.get(h.data_offset..h.chunk_end)?,
        // shortcut: level 0 only; GVR mip levels follow it when flagged.
        mip_count: 1,
        palette_format: 0,
        palette: &[],
    };
    let (width, height, pixels) = decode_mip_chain(&input).into_iter().next()?;
    Some(Rgba {
        width,
        height,
        pixels,
    })
}

/// Every texture of the archive; an entry that does not decode becomes a
/// 1×1 magenta texture so texture ids stay in place.
pub fn gvm_textures(prs: &[u8]) -> Result<Vec<Rgba>, String> {
    let gvm = archive::decompress_prs(prs, 64 * 1024 * 1024).map_err(|e| e.to_string())?;
    let entries = archive::read_gvm(&gvm).map_err(|e| e.to_string())?;
    Ok(entries
        .iter()
        .map(|e| {
            decode(e.data).unwrap_or(Rgba {
                width: 1,
                height: 1,
                pixels: vec![255, 0, 255, 255],
            })
        })
        .collect())
}
