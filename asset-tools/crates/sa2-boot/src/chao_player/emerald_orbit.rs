//! The seven Chaos Emeralds circling the player's own Chao, as the old hub's
//! owner aura did (archive/old-hub src/scene.js attachNativeEmeralds, last at
//! commit 420f1eb): SA2_EMERALDS=1, set by the page for the owner's account.
//!
//! All of it is the game's own data, read from the player's files:
//! - the models are the Last Story cutscene's (event/e0207.prs, scene 2,
//!   entities 1..=7: one node each, the same 124 vertices, texture ids 2..=8),
//!   their chunk vertex and polygon lists copied as they are into game heap 1;
//! - the textures are e0207texture.prs's first nine (0..=8, the seven
//!   chaoseme_* facets are 2..=8), handed to the game's texture loader as a
//!   file made in memory so the other 77 cutscene textures stay off the heap.
//! The game's own model draw applies the lists' material (0.8 alpha, both
//! sides, the camera-normal facet mapping).
use super::*;
use sa2_asset_tools::{archive::decompress_prs, event};

/// Pointer base of the retail event images (sa2_asset_tools::event).
const EVENT_BASE: u32 = 0x8125_FE60;
const SCENE: usize = 2;
const FIRST: usize = 1;
const COUNT: usize = 7;
/// Textures kept from e0207texture: ids 0..=8.
const TEX_KEEP: usize = 9;

// The old hub's orbit, in Chao heights: radius 0.4 and gem height 0.09 on a
// 0.5-high Chao, centre at 0.55 of its height, bob 0.035, 0.65 and 0.8 rad/s.
const RADIUS: f32 = 0.8;
const CENTRE: f32 = 0.55;
const BOB: f32 = 0.07;
const GEM: f32 = 0.18;
const ORBIT_RATE: f32 = 0.65;
const SPIN_RATE: f32 = 0.8;
/// shortcut: the Chao's height and its body node's (1) height in game units
/// are estimates, not measured from its model; SA2_EMERALD_H=<height> tries
/// another height. Upgrade: take both from the Chao's rest model.
const CHAO_HEIGHT: f32 = 3.2;
const BODY_NODE: f32 = 0.3;

thread_local! {
    /// The texture list, the seven objects and a gem's height; None when they could not load.
    static LOADED: Cell<Option<Option<(u32, [u32; COUNT], f32)>>> = const { Cell::new(None) };
}

/// The stage change freed heap 1's loads (drop_stage_loads).
pub(super) fn forget() {
    LOADED.set(None);
}

fn word(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Byte length of a chunk vertex list, its end chunk included.
fn vlist_len(b: &[u8], at: usize) -> Option<usize> {
    let mut a = at;
    loop {
        let h = word(b, a)?;
        if h & 0xFF == 0xFF {
            return Some(a + 4 - at);
        }
        a += 4 + 4 * (h >> 16) as usize;
    }
}

/// Byte length of a chunk polygon list, its end chunk included.
fn plist_len(b: &[u8], at: usize) -> Option<usize> {
    let half = |a: usize| Some(u16::from_be_bytes(b.get(a..a + 2)?.try_into().ok()?));
    let mut a = at;
    loop {
        a += match half(a)? & 0xFF {
            0xFF => return Some(a + 2 - at),
            0..=7 => 2,                                // null and bits chunks
            8..=15 => 4,                               // tiny (texture id)
            _ => 4 + 2 * usize::from(half(a + 2)?),    // material, volume, strip
        };
    }
}

/// One emerald as its file has it: vertex list, polygon list, centre and radius.
struct Gem<'a> {
    vlist: &'a [u8],
    plist: &'a [u8],
    bound: &'a [u8],
}

/// The seven emeralds of the expanded e0207 image.
fn gems(image: &[u8]) -> Option<Vec<Gem<'_>>> {
    let ev = event::read(image).ok()?;
    let at = |p: u32| p.checked_sub(EVENT_BASE).map(|v| v as usize);
    ev.scenes.get(SCENE)?.entities.get(FIRST..FIRST + COUNT)?.iter().map(|e| {
        let model = at(word(image, e.model_offset? + 4)?)?;
        let (v, p) = (at(word(image, model)?)?, at(word(image, model + 4)?)?);
        Some(Gem {
            vlist: image.get(v..v + vlist_len(image, v)?)?,
            plist: image.get(p..p + plist_len(image, p)?)?,
            bound: image.get(model + 8..model + 24)?,
        })
    }).collect()
}

/// A GVM archive cut down to its first `keep` textures.
fn cut_gvm(gvm: &[u8], keep: usize) -> Option<Vec<u8>> {
    let le = |a: usize| Some(u32::from_le_bytes(gvm.get(a..a + 4)?.try_into().ok()?) as usize);
    if gvm.get(..4)? != b"GVMH" || usize::from(u16::from_be_bytes(gvm.get(10..12)?.try_into().ok()?)) < keep {
        return None;
    }
    let mut a = 8 + le(4)?;
    for _ in 0..keep {
        // An optional global index chunk, then the texture.
        if gvm.get(a..a + 4)? == b"GBIX" {
            a += 8 + le(a + 4)?;
        }
        if gvm.get(a..a + 4)? != b"GVRT" {
            return None;
        }
        a += 8 + le(a + 4)?;
    }
    let mut out = gvm.get(..a)?.to_vec();
    out[10..12].copy_from_slice(&(keep as u16).to_be_bytes());
    Some(out)
}

/// `data` as a PRS stream of literal bytes only (no matching: the file is
/// small and made once a stage).
fn prs_literal(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 8 + 4);
    let mut groups = data.chunks_exact(8);
    for g in &mut groups {
        out.push(0xFF);
        out.extend_from_slice(g);
    }
    // The rest as literals, then the end: bits 0, 1 and a zero offset.
    let rest = groups.remainder();
    if rest.len() == 7 {
        out.push(0x7F);
        out.extend_from_slice(rest);
        out.push(0x01);
    } else {
        out.push(((1u8 << rest.len()) - 1) | (2 << rest.len()));
        out.extend_from_slice(rest);
    }
    out.extend_from_slice(&[0, 0]);
    out
}

/// Height of a position + normal vertex list (chunk 41: 24 bytes a vertex).
fn height(vlist: &[u8]) -> Option<f32> {
    let n = (word(vlist, 4)? >> 16) as usize;
    if word(vlist, 0)? & 0xFF != 41 || n == 0 {
        return None;
    }
    let ys = (0..n).map(|k| word(vlist, 12 + 24 * k).map(f32::from_bits)).collect::<Option<Vec<_>>>()?;
    Some(ys.iter().copied().fold(f32::MIN, f32::max) - ys.iter().copied().fold(f32::MAX, f32::min))
}

fn load(c: &mut Cpu, m: &mut Mem) -> Option<(u32, [u32; COUNT], f32)> {
    const LIMIT: usize = 64 << 20;
    let image = decompress_prs(&crate::dvd::read_file("e0207.prs")?, LIMIT).ok()?;
    let gems = gems(&image)?;
    let tall = height(gems[0].vlist).filter(|&h| h > 0.0)?;
    let gvm = decompress_prs(&crate::dvd::read_file("e0207texture.prs")?, LIMIT).ok()?;
    crate::dvd::add_file("CHAOSEME.PRS", prs_literal(&cut_gvm(&gvm, TEX_KEEP)?));
    let heap_var = c.r[13].wrapping_sub(SY_HEAP_SDA);
    let keep_heap = m.r32(heap_var);
    m.w32(heap_var, GEAR_HEAP);
    write_str(m, b"CHAOSEME");
    let tex = call(c, m, LOAD_TEX, &[STR]);
    m.w32(heap_var, keep_heap);
    if tex == 0 || (m.r32(tex + 4) as usize) < TEX_KEEP {
        return None;
    }
    // One vertex list when all seven share it (they do in the retail file).
    let shared = gems.iter().all(|g| g.vlist == gems[0].vlist);
    let pad = |n: usize| (n as u32 + 3) & !3;
    let (model, object) = (24u32, 0x34u32);
    let each = |g: &Gem| pad(g.plist.len()) + model + object + if shared { 0 } else { pad(g.vlist.len()) };
    let total = gems.iter().map(each).sum::<u32>() + if shared { pad(gems[0].vlist.len()) } else { 0 };
    let mem = call(c, m, OS_ALLOC_FROM_HEAP, &[GEAR_HEAP, total]);
    if mem == 0 {
        return None;
    }
    zero(m, mem, total);
    let mut a = mem;
    let mut put = |m: &mut Mem, bytes: &[u8]| {
        let at = a;
        m.load(at, bytes);
        a += pad(bytes.len());
        at
    };
    let first_vl = put(m, gems[0].vlist);
    let mut objs = [0; COUNT];
    for (i, g) in gems.iter().enumerate() {
        let vl = if shared || i == 0 { first_vl } else { put(m, g.vlist) };
        let pl = put(m, g.plist);
        let mdl = put(m, &[0; 24]);
        let o = put(m, &[0; 0x34]);
        m.w32(mdl, vl);
        m.w32(mdl + 4, pl);
        m.load(mdl + 8, g.bound);
        m.w32(o, 0x17); // UNIT_POS | UNIT_ANG | UNIT_SCL | BREAK (no child)
        m.w32(o + 4, mdl);
        for j in 0..3 { m.w32(o + 32 + 4 * j, 1.0f32.to_bits()); }
        objs[i] = o;
    }
    eprintln!("chao-player emeralds: textures {tex:#x}, {total} bytes of models at {mem:#x}");
    Some((tex, objs, tall))
}

/// Draw the seven around the Chao this draw just walked (its body node 1).
pub(super) fn draw(c: &mut Cpu, m: &mut Mem) {
    if std::env::var("SA2_EMERALDS").map_or(true, |v| v == "0") || debug_off("emeralds") {
        return;
    }
    let frame = sa2_recomp::host::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
    let Some(body) = NODE_MATS.with_borrow(|n| n.get(&1).copied()) else { return };
    if frame < GEAR_LOAD_AT.get() {
        return;
    }
    let loaded = LOADED.get().unwrap_or_else(|| {
        let l = load(c, m);
        if l.is_none() {
            eprintln!("chao-player emeralds: not loaded (e0207 files, textures or heap 1)");
        }
        LOADED.set(Some(l));
        l
    });
    let Some((tex, objs, tall)) = loaded else { return };
    let base = mtx(m, m.r32(NJ_BASE_MATRIX));
    let world = inv_mul(&base, &body);
    // The Chao's own scale (a child is smaller) times its height.
    let scale = (0..3).map(|r| world[r][0] * world[r][0]).sum::<f32>().sqrt();
    let h = scale * std::env::var("SA2_EMERALD_H").ok().and_then(|v| v.parse().ok()).unwrap_or(CHAO_HEIGHT);
    let size = GEM * h / tall;
    let t = frame as f32 / 60.0;
    let cur_ptr = m.r32(NJ_CURRENT_MATRIX);
    let keep_m = mtx(m, cur_ptr);
    let list = m.r32(c.r[13].wrapping_sub(0x7228)) + 0x20;
    let keep_list = m.r32(list);
    let regs = (c.r, c.f);
    let lcap = LCAP.replace(false);
    m.w32(list, tex);
    for (i, &obj) in objs.iter().enumerate() {
        let turn = i as f32 * std::f32::consts::TAU / COUNT as f32;
        let (phase, (s, co)) = (t * ORBIT_RATE + turn, (t * SPIN_RATE + turn).sin_cos());
        // World axes: around the body, upright, turning about its own height.
        let at = [
            [co * size, 0.0, s * size, world[0][3] + phase.cos() * RADIUS * h],
            [0.0, size, 0.0, world[1][3] + (CENTRE - BODY_NODE + (phase * 2.0).sin() * BOB) * h],
            [-s * size, 0.0, co * size, world[2][3] + phase.sin() * RADIUS * h],
        ];
        let place = mul34(&base, &at);
        for r in 0..3 { for q in 0..4 { m.w32(cur_ptr + 4 * (4 * r + q) as u32, place[r][q].to_bits()); } }
        call(c, m, 0x8011_DF44, &[obj]);
    }
    LCAP.set(lcap);
    (c.r, c.f) = regs;
    m.w32(list, keep_list);
    for r in 0..3 { for q in 0..4 { m.w32(cur_ptr + 4 * (4 * r + q) as u32, keep_m[r][q].to_bits()); } }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_prs_expands_to_its_input() {
        for n in [0usize, 1, 6, 7, 8, 9, 15, 16, 1000] {
            let data: Vec<u8> = (0..n).map(|i| (i * 7 + 3) as u8).collect();
            assert_eq!(decompress_prs(&prs_literal(&data), n + 8).unwrap(), data, "{n} bytes");
        }
    }

    #[test]
    fn a_cut_archive_keeps_the_first_textures() {
        let mut gvm = b"GVMH".to_vec();
        gvm.extend_from_slice(&8u32.to_le_bytes());
        gvm.extend_from_slice(&[0, 0xF, 0, 3, 0, 0, 0, 0]);
        for (i, len) in [4u32, 8, 4].into_iter().enumerate() {
            if i == 1 {
                gvm.extend_from_slice(b"GBIX");
                gvm.extend_from_slice(&4u32.to_le_bytes());
                gvm.extend_from_slice(&[9; 4]);
            }
            gvm.extend_from_slice(b"GVRT");
            gvm.extend_from_slice(&len.to_le_bytes());
            gvm.extend(std::iter::repeat_n(i as u8, len as usize));
        }
        let cut = cut_gvm(&gvm, 2).unwrap();
        assert_eq!(cut.len(), 16 + 12 + 12 + 16);
        assert_eq!(cut[10..12], [0, 2]);
        assert_eq!(cut[..10], gvm[..10]);
        assert!(cut_gvm(&gvm, 4).is_none());
    }

    #[test]
    fn list_lengths_include_the_end_chunk() {
        // One 2-word vertex chunk, then the end.
        let v = [0u8, 2, 0, 41, 0, 1, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0xFF];
        assert_eq!(vlist_len(&v, 0), Some(16));
        // Texture id, a 1-word material, a 2-word strip, the end.
        let p = [0u8, 8, 0x40, 3, 0, 17, 0, 1, 9, 9, 0, 64, 0, 2, 1, 1, 2, 2, 0, 0xFF];
        assert_eq!(plist_len(&p, 0), Some(20));
        assert_eq!(plist_len(&p[..18], 0), None);
    }

    /// SA2_EVENT_E0207=<the player's own event/e0207.prs>: the retail file's seven.
    #[test]
    fn the_retail_file_has_seven_emeralds_on_textures_2_to_8() {
        let Ok(path) = std::env::var("SA2_EVENT_E0207") else { return };
        let image = decompress_prs(&std::fs::read(path).unwrap(), 64 << 20).unwrap();
        let all = gems(&image).unwrap();
        assert_eq!(all.len(), 7);
        assert!((height(all[0].vlist).unwrap() - 2.986).abs() < 0.001);
        for (i, g) in all.iter().enumerate() {
            assert_eq!((g.vlist.len(), g.plist.len()), (2988, 586));
            assert_eq!(g.vlist, all[0].vlist);
            // After the 16-byte material chunk: the texture chunk's id word.
            assert_eq!(u16::from_be_bytes([g.plist[18], g.plist[19]]) & 0x1FFF, 2 + i as u16);
        }
    }
}
