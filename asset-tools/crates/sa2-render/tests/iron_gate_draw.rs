//! Iron Gate's landtable drawn offscreen on a native wgpu backend (needs
//! SA2_GAME_PATH and a GPU; nothing is committed). With SA2_RENDER_OUT the
//! frame is written there as a PNG to look at.
use sa2_asset_tools::stage_land;
use sa2_render::gpu::{look_at, Renderer};
use sa2_render::land::land_mesh;
use std::{fs, path::PathBuf};

const SIZE: [u32; 2] = [960, 540];

#[test]
#[ignore = "requires the installed Steam game (SA2_GAME_PATH) and a GPU"]
fn iron_gate_land_draws() {
    let game = PathBuf::from(std::env::var_os("SA2_GAME_PATH").expect("SA2_GAME_PATH"));
    let rel = fs::read(game.join("resource/gd_PC/stg11D.rel")).unwrap();
    let stage = stage_land::read(&rel).unwrap();
    eprintln!(
        "chunk {} basic {} formats {:?} visual_count {}",
        stage.chunk_attachments.len(),
        stage.basic_attachments.len(),
        stage
            .entries
            .iter()
            .map(|e| e.geometry_format.as_str())
            .collect::<std::collections::BTreeSet<_>>(),
        stage.visual_count
    );
    let row = stage_land::catalog()
        .iter()
        .find(|r| r.filename == "stg11D.rel")
        .unwrap();
    let rel_view = sa2_asset_tools::city_rel::CityRel::read_identified(&rel, row.module).unwrap();
    let gc = sa2_render::gc::GcSource {
        rel: &rel_view,
        section: row.section,
    };
    let (mesh, skipped) = land_mesh(&stage, Some(&gc));
    let (lo, hi) = mesh.bounds();
    eprintln!(
        "entries {} vertices {} triangles {} draws {} bounds {lo:?}..{hi:?} skipped {skipped:?}",
        stage.entries.len(),
        mesh.vertices.len(),
        mesh.indices.len() / 3,
        mesh.draws.len()
    );
    assert!(mesh.indices.len() > 3000);

    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle().with_env());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut r = Renderer::new(device, queue, format);
    r.set_meshes(&[&mesh]);
    let target = r.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("target"),
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
    // From above and behind the start (0,0,0), looking down the course.
    let vp = look_at(
        [-300.0, 400.0, 600.0],
        [600.0, -250.0, -400.0],
        1.0,
        SIZE[0] as f32 / SIZE[1] as f32,
        5.0,
        20000.0,
    );
    r.render(&view, SIZE, vp);

    let row = SIZE[0] * 4;
    let buf = r.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: u64::from(row * SIZE[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = r.device.create_command_encoder(&Default::default());
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
    r.queue.submit([enc.finish()]);
    buf.slice(..)
        .map_async(wgpu::MapMode::Read, |res| res.unwrap());
    r.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let pixels = buf.slice(..).get_mapped_range().unwrap().to_vec();
    let bg = [pixels[0], pixels[1], pixels[2]]; // a corner shows the clear color
    let drawn = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[..3].iter().zip(bg).any(|(a, b)| a.abs_diff(b) > 6))
        .count();
    eprintln!("drawn pixels {drawn} of {}", SIZE[0] * SIZE[1]);
    if let Some(out) = std::env::var_os("SA2_RENDER_OUT") {
        let f = fs::File::create(out).unwrap();
        let mut e = png::Encoder::new(f, SIZE[0], SIZE[1]);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
    assert!(
        drawn > (SIZE[0] * SIZE[1] / 20) as usize,
        "the land covers part of the frame"
    );
}
