//! Stage textures: a PRS-packed GVM archive of GVR textures (Iron Gate's
//! `landtx11.prs` on Steam), decoded by the sa2 crate, in archive order (the
//! order of the stage's texture list).
use crate::gpu::Rgba;
use sa2_asset_tools::{archive, texture};

/// Every texture of the archive; an entry that does not decode becomes a
/// 1×1 magenta texture so texture ids stay in place.
pub fn gvm_textures(prs: &[u8]) -> Result<Vec<Rgba>, String> {
    let gvm = archive::decompress_prs(prs, 64 * 1024 * 1024).map_err(|e| e.to_string())?;
    let entries = archive::read_gvm(&gvm).map_err(|e| e.to_string())?;
    Ok(entries
        .iter()
        .map(|e| match texture::decode_gvr(e.data, None) {
            Ok(i) => Rgba {
                width: i.width,
                height: i.height,
                pixels: i.rgba,
            },
            Err(_) => Rgba {
                width: 1,
                height: 1,
                pixels: vec![255, 0, 255, 255],
            },
        })
        .collect())
}
