//! Offscreen snapshots of the game's GX frames for headless runs: every
//! `every`-th frame is drawn at 640x480 and written as a PNG. Texture uploads
//! between snapshots are kept as the latest data per texture id and reach
//! the GPU only when a snapshot is drawn.
use sa2_gx::api::{Frame, TextureUpload};
use sa2_gx::gpu::GxRenderer;
use sa2_gx::params::TextureId;
use sa2_recomp::{global, global::Global};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const W: u32 = 640;
const H: u32 = 480;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

type Levels = Vec<(u32, u32, Vec<u8>)>;

struct Snap {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: GxRenderer,
    target: wgpu::Texture,
    buf: wgpu::Buffer,
    /// Uploads since the last snapshot, newest per id.
    pending: HashMap<TextureId, Levels>,
}

/// Keep the newest data for each uploaded texture. The game re-uploads every
/// texture every frame; making a GPU texture for each (about 160 a frame) grew
/// the process by ~8 MB every 10 s in the driver, outside wgpu's own memory.
fn note_uploads(pending: &mut HashMap<TextureId, Levels>, uploads: &[TextureUpload]) {
    for u in uploads {
        pending.insert(u.id, u.levels.clone());
    }
}

static SNAP: Global<Option<Snap>> = global!(None);

fn open() -> Result<Snap, String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        ..Default::default()
    }))
    .map_err(|e| format!("no wgpu adapter: {e}"))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_limits:
            wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
        ..Default::default()
    }))
    .map_err(|e| format!("no wgpu device: {e}"))?;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("snapshot"),
        size: wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("snapshot readback"),
        size: u64::from(W * 4 * H),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let renderer = GxRenderer::new(device.clone(), queue.clone(), FORMAT);
    Ok(Snap {
        device,
        queue,
        renderer,
        target,
        buf,
        pending: HashMap::new(),
    })
}

/// Feed frame number `n`; on every `every`-th frame write `dir/frame_NNNNNN.png`
/// and return its path.
pub fn frame(f: &Frame, n: u64, every: u64, dir: &Path) -> Result<Option<PathBuf>, String> {
    SNAP.with_borrow_mut(|s| {
        if s.is_none() {
            *s = Some(open()?);
        }
        let Some(s) = s.as_mut() else { return Ok(None) };
        note_uploads(&mut s.pending, &f.uploads);
        if every == 0 || !n.is_multiple_of(every) {
            return Ok(None);
        }
        for (id, levels) in s.pending.drain() {
            s.renderer.replace_texture(id, &levels);
        }
        if std::env::var_os("SA2_SNAP_INFO").is_some() {
            eprintln!("frame {n}: clear {:?}, {} draws, {} uploads", f.clear_color, f.draws.len(), f.uploads.len());
            let mut seen = std::collections::BTreeSet::new();
            for d in &f.draws {
                let key = format!("{:?} {:?}", d.viewport, d.projection);
                if seen.insert(key) && seen.len() <= 6 {
                    eprintln!("  viewport {:?} scissor {:?} proj {:?} verts {}B idx {}", d.viewport, d.scissor, d.projection, d.vertices.len(), d.indices.len());
                }
            }
            eprintln!("  {} distinct viewport/projection pairs", seen.len());
        }
        let view = s.target.create_view(&Default::default());
        s.renderer.render_frame(f, &view, [W, H]);
        let mut enc = s.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            s.target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &s.buf,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(W * 4), rows_per_image: None },
            },
            wgpu::Extent3d { width: W, height: H, depth_or_array_layers: 1 },
        );
        s.queue.submit([enc.finish()]);
        s.buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        s.device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| e.to_string())?;
        let px = s.buf.slice(..).get_mapped_range().map_err(|e| format!("{e:?}"))?.to_vec();
        s.buf.unmap();
        // SA2_GPU_REPORT: what the snapshot device holds (finding leaks).
        if std::env::var_os("SA2_GPU_REPORT").is_some() {
            let [p, t, sm, b] = s.renderer.cache_counts();
            let a = s.device.generate_allocator_report();
            let (alloc, reserved, blocks, count) = a.map_or((0, 0, 0, 0), |a| {
                (a.total_allocated_bytes >> 20, a.total_reserved_bytes >> 20, a.blocks.len(), a.allocations.len())
            });
            eprintln!("gpu: frame {n} pipelines {p} textures {t} samplers {sm} binds {b} | allocated {alloc} MB reserved {reserved} MB in {blocks} blocks, {count} allocations");
        }
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!("frame_{n:06}.png"));
        let file = std::io::BufWriter::new(std::fs::File::create(&path).map_err(|e| e.to_string())?);
        let mut enc = png::Encoder::new(file, W, H);
        // RGB: the EFB has no alpha the game cares about (it writes 0).
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().map_err(|e| e.to_string())?;
        let rgb: Vec<u8> = px.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
        w.write_image_data(&rgb).map_err(|e| e.to_string())?;
        Ok(Some(path))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upload(id: TextureId, v: u8) -> TextureUpload {
        TextureUpload {
            id,
            levels: vec![(1, 1, vec![v; 4])],
        }
    }

    #[test]
    fn pending_uploads_stay_one_per_texture() {
        // 1000 frames re-uploading the same 3 textures, as the game does.
        let mut pending = HashMap::new();
        for frame in 0..1000u32 {
            let v = (frame % 256) as u8;
            note_uploads(&mut pending, &[upload(1, v), upload(2, v), upload(7, v)]);
        }
        assert_eq!(pending.len(), 3);
        assert_eq!(
            pending[&7][0].2,
            vec![(999 % 256) as u8; 4],
            "the newest data wins"
        );
    }
}
