//! Mech Eggman's walker model (`ewalkmdl.prs`) triangulated in its bind pose
//! (needs SA2_GAME_PATH; nothing from the game is committed).
use sa2_asset_tools::model_bank;
use sa2_render::land::model_mesh;
use std::{fs, path::PathBuf};

#[test]
#[ignore = "requires the installed Steam game (SA2_GAME_PATH)"]
fn walker_bank_triangulates() {
    let game = PathBuf::from(std::env::var_os("SA2_GAME_PATH").expect("SA2_GAME_PATH"));
    let prs = fs::read(game.join("resource/gd_PC/ewalkmdl.prs")).unwrap();
    let bank = model_bank::read_prs(&prs).unwrap();
    assert!(!bank.is_empty());
    for row in &bank {
        let (mesh, skipped) = model_mesh(&row.model);
        let (lo, hi) = mesh.bounds();
        let kinds: std::collections::BTreeSet<u8> = row
            .model
            .attachments
            .iter()
            .flat_map(|a| a.vertex_chunks.iter().map(|v| v.kind))
            .collect();
        eprintln!(
            "id {:3} nodes {:3} tris {:5} draws {:3} vkinds {kinds:?} bounds {lo:?}..{hi:?} skipped {skipped:?}",
            row.id,
            row.model.nodes.len(),
            mesh.indices.len() / 3,
            mesh.draws.len()
        );
    }
    let body = bank.iter().find(|r| r.id == 248).expect("row 248");
    let (body, skipped) = model_mesh(&body.model);
    assert!(body.indices.len() / 3 > 1500 && skipped.is_empty());
}

/// The walker body (row 248, the model the action table draws) with its textures, drawn offscreen from close by.
/// With SA2_RENDER_OUT the frame is written there as a PNG.
#[test]
#[ignore = "requires the installed Steam game (SA2_GAME_PATH) and a GPU"]
fn walker_body_draws() {
    use sa2_render::gpu::{look_at, Renderer};
    const SIZE: [u32; 2] = [640, 640];
    let game = PathBuf::from(std::env::var_os("SA2_GAME_PATH").expect("SA2_GAME_PATH"));
    let gd = game.join("resource/gd_PC");
    let bank = model_bank::read_prs(&fs::read(gd.join("ewalkmdl.prs")).unwrap()).unwrap();
    let row = bank.iter().find(|r| r.id == 248).unwrap();
    let (mesh, _) = model_mesh(&row.model);
    let tex =
        sa2_render::textures::gvm_textures(&fs::read(gd.join("ewalktex.prs")).unwrap()).unwrap();
    let max_id = mesh.draws.iter().filter_map(|d| d.texture).max();
    eprintln!("walker textures {} highest id used {max_id:?}", tex.len());
    assert!(max_id.is_none_or(|m| usize::from(m) < tex.len()));

    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle().with_env());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut r = Renderer::new(device, queue, format);
    r.set_meshes(&[&mesh]);
    r.set_textures(&tex);
    let target = r.gx.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: SIZE[0],
            height: SIZE[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    r.render(
        &view,
        SIZE,
        look_at([18.0, 14.0, 22.0], [0.0, 4.0, 0.0], 1.0, 1.0, 0.5, 500.0),
    );
    let row = SIZE[0] * 4;
    let buf = r.gx.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * SIZE[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = r.gx.device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(SIZE[1]),
            },
        },
        wgpu::Extent3d {
            width: SIZE[0],
            height: SIZE[1],
            depth_or_array_layers: 1,
        },
    );
    r.gx.queue.submit([enc.finish()]);
    buf.slice(..)
        .map_async(wgpu::MapMode::Read, |res| res.unwrap());
    r.gx.device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let pixels = buf.slice(..).get_mapped_range().unwrap().to_vec();
    let bg = [pixels[0], pixels[1], pixels[2]];
    let drawn = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[..3].iter().zip(bg).any(|(a, b)| a.abs_diff(b) > 6))
        .count();
    eprintln!("drawn pixels {drawn}");
    if let Some(out) = std::env::var_os("SA2_RENDER_OUT") {
        let mut e = png::Encoder::new(fs::File::create(out).unwrap(), SIZE[0], SIZE[1]);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
    assert!(
        drawn > (SIZE[0] * SIZE[1] / 20) as usize,
        "the walker fills part of the frame"
    );
}

/// The model ids Mech Eggman's action table (in sonic2app.exe) draws.
#[test]
#[ignore = "requires the installed Steam game (SA2_GAME_PATH)"]
fn walker_action_models() {
    let game = PathBuf::from(std::env::var_os("SA2_GAME_PATH").expect("SA2_GAME_PATH"));
    let exe = fs::read(game.join("sonic2app.exe")).unwrap();
    let t = sa2_asset_tools::character_animations::read(&exe).unwrap();
    let t = t.into_iter().find(|t| t.character == "MechEggman").unwrap();
    let mut ids = std::collections::BTreeMap::<i64, usize>::new();
    for r in &t.rows {
        *ids.entry(i64::from(r.model)).or_default() += 1;
    }
    eprintln!("action rows {} model ids {ids:?}", t.rows.len());
    assert!(ids.contains_key(&248));
}

/// Which installed model banks hold a given id (SA2_FIND_ID, default 248).
#[test]
#[ignore = "requires the installed Steam game (SA2_GAME_PATH)"]
fn find_model_id() {
    let game = PathBuf::from(std::env::var_os("SA2_GAME_PATH").expect("SA2_GAME_PATH"));
    let want: i32 = std::env::var("SA2_FIND_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(248);
    for e in fs::read_dir(game.join("resource/gd_PC")).unwrap() {
        let p = e.unwrap().path();
        let n = p.file_name().unwrap().to_string_lossy().to_lowercase();
        if !n.ends_with("mdl.prs") {
            continue;
        }
        if let Ok(bank) = model_bank::read_prs(&fs::read(&p).unwrap()) {
            if let Some(r) = bank.iter().find(|r| r.id == want) {
                eprintln!("{n}: id {want} nodes {}", r.model.nodes.len());
            }
        }
    }
}
