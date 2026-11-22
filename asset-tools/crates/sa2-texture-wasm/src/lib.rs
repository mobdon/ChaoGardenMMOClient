//! `game-texture.wasm`: one named texture of a game `.prs` texture archive
//! (PRS -> GVM -> GVR, the sa2 crate's own readers) as a PNG. The page reads the
//! archive from the player's own game folder; nothing is fetched from a server.
//! JS calls `buf_alloc`, copies the archive and the texture name in, calls `convert`,
//! reads `out_ptr`/`out_len`, then `buf_free`s both buffers. `chao` does the same for one
//! Chao model with one motion as a GLB, from `Data_DLL.dll`, `al_body.prs` and the palette files.
//! Build: `cargo build --release --target wasm32-unknown-unknown`.

use sa2_asset_tools::chao_palette::Ratios;
use std::cell::RefCell;

/// Largest decompressed archive we accept (the readers' own bound elsewhere).
const MAX_ARCHIVE: usize = 16 << 20;

thread_local! {
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn png(prs: &[u8], name: &str) -> sa2_asset_tools::Result<Vec<u8>> {
    let data = sa2_asset_tools::archive::decompress_prs(prs, MAX_ARCHIVE)?;
    let entries = sa2_asset_tools::archive::read_gvm(&data)?;
    let entry = entries
        .iter()
        .find(|e| e.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| sa2_asset_tools::Error::invalid("texture name not in archive"))?;
    let mut out = Vec::new();
    sa2_asset_tools::texture::decode_gvr(entry.data, None)?.write_png(&mut out)?;
    Ok(out)
}

/// Converts texture `name` of `prs`; the PNG (or nothing on error) is kept for `out_ptr`/`out_len`.
pub fn convert_to_out(prs: &[u8], name: &str) -> bool {
    let made = png(prs, name);
    let ok = made.is_ok();
    OUT.with(|o| *o.borrow_mut() = made.unwrap_or_default());
    ok
}

#[no_mangle]
pub extern "C" fn buf_alloc(len: usize) -> *mut u8 {
    let mut v = vec![0u8; len.max(1)];
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `ptr`/`len` must come from one `buf_alloc` call.
#[no_mangle]
pub unsafe extern "C" fn buf_free(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, len.max(1), len.max(1)));
}

/// # Safety
/// Both pointer/length pairs must be `buf_alloc`ed buffers: the archive bytes and the UTF-8 name.
#[no_mangle]
pub unsafe extern "C" fn convert(
    prs: *const u8,
    prs_len: usize,
    name: *const u8,
    name_len: usize,
) -> i32 {
    let name = std::str::from_utf8(std::slice::from_raw_parts(name, name_len));
    name.is_ok_and(|name| convert_to_out(std::slice::from_raw_parts(prs, prs_len), name)) as i32
}

/// The Chao model `AL_RootObject[model]` of `Data_DLL.dll` with motion `al_motion_table[motion]`
/// (named `motion`) as a GLB, textured from `al_body.prs` with the game's blend of the 39
/// palette files (`gvps`, concatenated in `chao_palette::SLOT_FILES` order) for `ratios`.
fn chao_glb(
    dll: &[u8],
    body: &[u8],
    gvps: &[u8],
    model: usize,
    motion: usize,
    ratios: &Ratios,
    family: bool,
) -> sa2_asset_tools::Result<Vec<u8>> {
    use sa2_asset_tools::{chao_dll, chao_glb, chao_palette};
    let index = model;
    let model = chao_dll::read_chao_model(dll, model)?;
    let clip = chao_dll::read_chao_motion(dll, motion)?;
    let slots = chao_palette::read_slots(gvps)?;
    let pngs = chao_glb::body_textures(&model, body, &slots, ratios)?;
    if family {
        // The whole body family as morph targets, face parts kept and tagged for the page.
        let names = chao_glb::family_targets(index)?;
        let models = names
            .iter()
            .map(|(_, i)| chao_dll::read_chao_model(dll, *i))
            .collect::<sa2_asset_tools::Result<Vec<_>>>()?;
        let targets: Vec<_> = names.iter().map(|n| n.0.as_str()).zip(&models).collect();
        let family = chao_glb::Family {
            name: "family",
            targets: &targets,
            texture_names: chao_glb::body_texture_names(body)?,
        };
        return chao_glb::export_family(&model, &pngs, &[("motion", &clip)], &[], &family);
    }
    // shortcut: every portrait Chao gets open eyes and mouth 0; pass the face numbers in when a scene needs others.
    chao_glb::export(
        &model,
        &pngs,
        &[("motion", &clip)],
        &chao_glb::OPEN_EYES_NO_MOUTH,
    )
}

/// Converts one Chao; the GLB (or nothing on error) is kept for `out_ptr`/`out_len`.
pub fn chao_to_out(
    dll: &[u8],
    body: &[u8],
    gvps: &[u8],
    model: usize,
    motion: usize,
    ratios: &Ratios,
    family: bool,
) -> bool {
    let made = chao_glb(dll, body, gvps, model, motion, ratios, family);
    let ok = made.is_ok();
    OUT.with(|o| *o.borrow_mut() = made.unwrap_or_default());
    ok
}

/// `chao_type`, `h_pos`, `v_pos`, `a_pos` and `growth` are the Chao's own values (`Ratios`).
/// A non-zero `family` writes the body family that starts at `model` as one morphing model.
///
/// # Safety
/// The three pointer/length pairs must be `buf_alloc`ed buffers: `Data_DLL.dll`, `al_body.prs`
/// and the 39 palette files.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn chao(
    dll: *const u8,
    dll_len: usize,
    body: *const u8,
    body_len: usize,
    gvps: *const u8,
    gvps_len: usize,
    model: usize,
    motion: usize,
    chao_type: u32,
    h_pos: f32,
    v_pos: f32,
    a_pos: f32,
    growth: f32,
    family: u32,
) -> i32 {
    let bytes = |p, n| std::slice::from_raw_parts(p, n);
    let ratios = Ratios {
        chao_type: chao_type as u8,
        h_pos,
        v_pos,
        a_pos,
        growth,
    };
    chao_to_out(
        bytes(dll, dll_len),
        bytes(body, body_len),
        bytes(gvps, gvps_len),
        model,
        motion,
        &ratios,
        family != 0,
    ) as i32
}

#[no_mangle]
pub extern "C" fn out_ptr() -> *const u8 {
    OUT.with(|o| o.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn out_len() -> usize {
    OUT.with(|o| o.borrow().len())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Synthetic test input (never game data): a literal-only PRS stream.
    fn prs_literal(data: &[u8]) -> Vec<u8> {
        let (mut out, mut flag_at, mut bit) = (Vec::new(), 0, 8);
        // Control bits: 1 per literal byte, then 0,1 and a zero offset word to end.
        let bits = data
            .iter()
            .map(|b| (1, Some(*b)))
            .chain([(0, None), (1, None)]);
        for (flag, byte) in bits {
            if bit == 8 {
                (flag_at, bit) = (out.len(), 0);
                out.push(0);
            }
            out[flag_at] |= flag << bit;
            bit += 1;
            out.extend(byte);
        }
        out.extend([0, 0]);
        out
    }

    // Synthetic one-texture GVM: a 4x4 ARGB8888 GVR named `name`.
    fn gvm(name: &[u8], pixels: &[u8; 64]) -> Vec<u8> {
        let mut b = b"GVMH".to_vec();
        b.extend(34u32.to_le_bytes());
        b.extend(1u16.to_be_bytes()); // names only
        b.extend(1u16.to_be_bytes());
        b.extend(0u16.to_be_bytes());
        let mut field = [0u8; 28];
        field[..name.len()].copy_from_slice(name);
        b.extend(field);
        b.extend(b"GVRT");
        b.extend(((pixels.len() + 8) as u32).to_le_bytes());
        b.extend([0, 0, 0, 6]);
        b.extend(4u16.to_be_bytes());
        b.extend(4u16.to_be_bytes());
        b.extend(pixels);
        b
    }

    #[test]
    fn named_texture_becomes_a_png_with_its_pixels() {
        let mut pixels = [0u8; 64];
        (pixels[0], pixels[1], pixels[32], pixels[33]) = (17, 22, 33, 44);
        let prs = prs_literal(&gvm(b"level00", &pixels));
        assert!(convert_to_out(&prs, "LEVEL00"));
        let bytes = OUT.with(|o| o.borrow().clone());
        let mut reader = png::Decoder::new(bytes.as_slice()).read_info().unwrap();
        let mut rgba = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut rgba).unwrap();
        assert_eq!((info.width, info.height), (4, 4));
        assert_eq!(&rgba[..4], &[22, 33, 44, 17]);
    }

    #[test]
    fn unknown_name_and_bad_bytes_give_nothing() {
        let prs = prs_literal(&gvm(b"level00", &[0; 64]));
        assert!(!convert_to_out(&prs, "level01"));
        assert_eq!(out_len(), 0);
        assert!(!convert_to_out(&[0u8; 16], "level00"));
        assert_eq!(out_len(), 0);
    }

    #[test]
    fn bad_chao_inputs_give_nothing() {
        assert!(!chao_to_out(
            &[0u8; 64],
            &[0u8; 16],
            &[0u8; 16],
            0,
            0,
            &Ratios::CHILD,
            true
        ));
        assert_eq!(out_len(), 0);
    }
}
