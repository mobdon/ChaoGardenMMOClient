//! Steam's PC texture packs (`gd_PC/PRS/<name>.pak`) served as the GameCube
//! texture archive the game's code reads (`<name>.prs`: a PRS-packed GVM).
//!
//! Layout, from the installed files: a `.pak` is an index (per entry: long
//! name, short name, size twice) followed by the data in index order. Each
//! pak holds one DDS per texture and `<name>.inf`, 60 bytes per texture in
//! GVM order: name[28], global index, GC format, 0, GC format, width,
//! height, data size, flags (LE). The GVM skeleton (entry order, names,
//! global indexes, header flags) comes from Steam's own GameCube-format
//! `<name>.prs` next to it; only the pixels come from the DDS files, at
//! Steam's resolution (472 of 2,025 Steam textures are twice the GC size).
//! The GC format comes from the `.inf`. For CMPR, DXT1 carries over
//! losslessly (same block coding, GC tile order and bit order) and DXT5
//! keeps its colour blocks (its alpha drops, as the GC texture is CMPR too);
//! other formats become RGB5A3. A DDS smaller than the GC texture (Steam's
//! 4x4 stubs), or 32-bit art where the GC texture is CMPR, keeps the GC
//! texture from Steam's own `.prs`.
//! shortcut: mip levels are kept only when the DDS chain reaches 1x1 (else level 0 alone); RGB5A3 quantizes Steam's 8-bit colour (try RGBA8 if the game's loader takes it).
use sa2_asset_tools::archive::decompress_prs;

const CMPR: u8 = 14;
const RGB5A3: u8 = 5;
/// 0x811ffe60 - 0x80fffe60: the archive loader's scratch (0x8001a190).
const SCRATCH: usize = 0x20_0000;

fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// The pak's entries as (short name, data).
pub fn read_pak(pak: &[u8]) -> Option<Vec<(String, &[u8])>> {
    if pak.get(1..4)? != b"pak" {
        return None;
    }
    let count = le32(pak, 0x39)? as usize;
    let mut p = 0x3D;
    let mut index = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let long = le32(pak, p)? as usize;
        p = p.checked_add(4 + long)?;
        let short = le32(pak, p)? as usize;
        let name = String::from_utf8_lossy(pak.get(p + 4..p + 4 + short)?).into_owned();
        p += 4 + short;
        index.push((name, le32(pak, p)? as usize));
        p += 8;
    }
    let mut out = Vec::with_capacity(index.len());
    for (name, len) in index {
        out.push((name, pak.get(p..p.checked_add(len)?)?));
        p += len;
    }
    (p == pak.len()).then_some(out)
}

/// One decoded DDS: size and its mip chain (level 0 first), each level as
/// RGBA8 pixels or, for DXT1, the raw 8-byte blocks.
struct Dds {
    w: usize,
    h: usize,
    kind: Kind,
    levels: Vec<Vec<u8>>,
}

/// How a DDS level is stored: DXT1 or DXT5 blocks as in the file, or RGBA8.
#[derive(PartialEq)]
enum Kind {
    Dxt1,
    Dxt5,
    Rgba,
}

fn read_dds(d: &[u8]) -> Option<Dds> {
    if d.get(..4)? != b"DDS " {
        return None;
    }
    let (h, w) = (le32(d, 12)? as usize, le32(d, 16)? as usize);
    let mips = (le32(d, 28)? as usize).max(1);
    let four = d.get(84..88)?;
    let bits = le32(d, 88)?;
    let masks = [le32(d, 92)?, le32(d, 96)?, le32(d, 100)?, le32(d, 104)?];
    let mut p = 128;
    let mut levels = Vec::new();
    for l in 0..mips {
        let (lw, lh) = ((w >> l).max(1), (h >> l).max(1));
        let blocks = lw.div_ceil(4) * lh.div_ceil(4);
        let level = match four {
            b"DXT1" => {
                let n = blocks * 8;
                let b = d.get(p..p + n)?.to_vec();
                p += n;
                b
            }
            b"DXT5" => {
                let n = blocks * 16;
                let b = d.get(p..p + n)?.to_vec();
                p += n;
                b
            }
            [0, 0, 0, 0] if bits == 32 => {
                let n = lw * lh * 4;
                let src = d.get(p..p + n)?;
                p += n;
                let chan = |px: u32, m: u32| {
                    if m == 0 {
                        255
                    } else {
                        ((px & m) >> m.trailing_zeros()) as u8
                    }
                };
                src.chunks(4)
                    .flat_map(|c| {
                        let px = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                        masks.map(|m| chan(px, m))
                    })
                    .collect()
            }
            _ => return None,
        };
        levels.push(level);
    }
    Some(Dds {
        w,
        h,
        kind: match four {
            b"DXT1" => Kind::Dxt1,
            b"DXT5" => Kind::Dxt5,
            _ => Kind::Rgba,
        },
        levels,
    })
}

fn rgb565(c: u16) -> [u8; 3] {
    let (r, g, b) = ((c >> 11) & 31, (c >> 5) & 63, c & 31);
    [
        ((r << 3) | (r >> 2)) as u8,
        ((g << 2) | (g >> 4)) as u8,
        ((b << 3) | (b >> 2)) as u8,
    ]
}

fn decode_dxt5(src: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    for (bi, b) in src.chunks(16).enumerate() {
        let (bx, by) = ((bi % w.div_ceil(4)) * 4, (bi / w.div_ceil(4)) * 4);
        let (a0, a1) = (u32::from(b[0]), u32::from(b[1]));
        let abits = b[2..8]
            .iter()
            .rev()
            .fold(0u64, |a, x| (a << 8) | u64::from(*x));
        let alpha = |i: u64| -> u8 {
            let i = i as u32;
            (match (i, a0 > a1) {
                (0, _) => a0,
                (1, _) => a1,
                (i, true) => ((8 - i) * a0 + (i - 1) * a1) / 7,
                (6, false) => 0,
                (7, false) => 255,
                (i, false) => ((6 - i) * a0 + (i - 1) * a1) / 5,
            }) as u8
        };
        let (c0, c1) = (
            u16::from_le_bytes([b[8], b[9]]),
            u16::from_le_bytes([b[10], b[11]]),
        );
        let (p0, p1) = (rgb565(c0), rgb565(c1));
        let mix =
            |a: u8, b: u8, ka: u16, kb: u16| ((u16::from(a) * ka + u16::from(b) * kb) / 3) as u8;
        let pal = [
            p0,
            p1,
            [
                mix(p0[0], p1[0], 2, 1),
                mix(p0[1], p1[1], 2, 1),
                mix(p0[2], p1[2], 2, 1),
            ],
            [
                mix(p0[0], p1[0], 1, 2),
                mix(p0[1], p1[1], 1, 2),
                mix(p0[2], p1[2], 1, 2),
            ],
        ];
        let cbits = u32::from_le_bytes([b[12], b[13], b[14], b[15]]);
        for i in 0..16 {
            let (x, y) = (bx + i % 4, by + i / 4);
            if x < w && y < h {
                let c = pal[((cbits >> (2 * i)) & 3) as usize];
                let o = (y * w + x) * 4;
                out[o..o + 4].copy_from_slice(&[c[0], c[1], c[2], alpha((abits >> (3 * i)) & 7)]);
            }
        }
    }
    out
}

/// DXT5 blocks' colour halves as DXT1 blocks that stay in 4-colour mode
/// (CMPR, like DXT1, reads c0 <= c1 as 3 colours + transparent): swap the
/// endpoints and flip index bit 0 when c0 < c1; equal endpoints are one colour.
fn dxt5_colour_blocks(dxt5: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(dxt5.len() / 2);
    for b in dxt5.chunks(16) {
        let c = &b[8..16];
        let (c0, c1) = (
            u16::from_le_bytes([c[0], c[1]]),
            u16::from_le_bytes([c[2], c[3]]),
        );
        if c0 > c1 {
            out.extend_from_slice(c);
        } else if c0 < c1 {
            out.extend_from_slice(&[c[2], c[3], c[0], c[1]]);
            out.extend(c[4..8].iter().map(|x| x ^ 0x55));
        } else {
            out.extend_from_slice(&[c[0], c[1], c[2], c[3], 0, 0, 0, 0]);
        }
    }
    out
}

/// DXT1 blocks (row-major) to CMPR: 8x8 tiles of 2x2 blocks, colours BE,
/// index rows with the 2-bit pairs in reverse order.
fn dxt1_to_cmpr(blocks: &[u8], w: usize, h: usize) -> Vec<u8> {
    let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
    let mut out = Vec::new();
    for ty in (0..bh.max(2)).step_by(2) {
        for tx in (0..bw.max(2)).step_by(2) {
            for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                let (x, y) = (tx + dx, ty + dy);
                if x >= bw || y >= bh {
                    out.extend_from_slice(&[0; 8]);
                    continue;
                }
                let b = &blocks[(y * bw + x) * 8..][..8];
                out.extend_from_slice(&[b[1], b[0], b[3], b[2]]);
                for r in &b[4..8] {
                    let s = (r & 3) << 6 | (r >> 2 & 3) << 4 | (r >> 4 & 3) << 2 | (r >> 6 & 3);
                    out.push(s);
                }
            }
        }
    }
    out
}

/// RGBA8 pixels to RGB5A3 in 4x4 tiles (BE).
fn rgba_to_rgb5a3(px: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for ty in (0..h.max(4)).step_by(4) {
        for tx in (0..w.max(4)).step_by(4) {
            for y in ty..ty + 4 {
                for x in tx..tx + 4 {
                    let v: u16 = if x < w && y < h {
                        let p = &px[(y * w + x) * 4..][..4];
                        let [r, g, b, a] = [p[0], p[1], p[2], p[3]].map(u16::from);
                        if a >= 0xE0 {
                            0x8000 | (r >> 3) << 10 | (g >> 3) << 5 | (b >> 3)
                        } else {
                            (a >> 5) << 12 | (r >> 4) << 8 | (g >> 4) << 4 | (b >> 4)
                        }
                    } else {
                        0
                    };
                    out.extend_from_slice(&v.to_be_bytes());
                }
            }
        }
    }
    out
}

fn dims_code(n: usize) -> u8 {
    (n.trailing_zeros() as u8).saturating_sub(2)
}

/// PRS stream that only copies literals (the game's own PRS reader unpacks it).
pub fn prs_literal(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() * 9 / 8 + 4);
    let mut flag_at = 0;
    let mut bit = 8;
    let push_bit = |out: &mut Vec<u8>, b: u8, flag_at: &mut usize, bit: &mut u32| {
        if *bit == 8 {
            *flag_at = out.len();
            out.push(0);
            *bit = 0;
        }
        out[*flag_at] |= b << *bit;
        *bit += 1;
    };
    for &b in data {
        push_bit(&mut out, 1, &mut flag_at, &mut bit);
        out.push(b);
    }
    // End: long copy (bits 0,1) with offset word 0.
    push_bit(&mut out, 0, &mut flag_at, &mut bit);
    push_bit(&mut out, 1, &mut flag_at, &mut bit);
    out.extend_from_slice(&[0, 0]);
    out
}

/// The GameCube `.prs` for Steam pak `pak`, built on Steam's own
/// GameCube-format `skeleton_prs` of the same name.
pub fn pak_to_prs(pak: &[u8], skeleton_prs: &[u8]) -> Option<Vec<u8>> {
    let gvm = decompress_prs(skeleton_prs, 64 << 20).ok()?;
    if gvm.get(..4)? != b"GVMH" {
        return None;
    }
    let header = le32(&gvm, 4)? as usize;
    let count = u16::from_be_bytes(gvm.get(10..12)?.try_into().ok()?) as usize;
    let files = read_pak(pak)?;
    let inf = files
        .iter()
        .find(|(n, _)| n.to_lowercase().ends_with(".inf"))?
        .1;
    if inf.len() != count * 60 {
        return None;
    }
    let mut chunks = Vec::with_capacity(count);
    let mut p = 8 + header;
    for _ in 0..count {
        let len = 8 + le32(&gvm, p + 4)? as usize;
        chunks.push(gvm.get(p..p + len)?);
        p += len;
    }
    let mut out = gvm.get(..8 + header)?.to_vec();
    for i in 0..count {
        let rec = &inf[i * 60..][..60];
        let name = String::from_utf8_lossy(&rec[..28])
            .trim_end_matches('\0')
            .to_lowercase();
        let entry = 12 + i * 38;
        let gc_name = String::from_utf8_lossy(out.get(entry + 2..entry + 30)?)
            .trim_end_matches('\0')
            .to_lowercase();
        if gc_name != name {
            return None;
        }
        let dds = files.iter().find(|(n, _)| {
            n.to_lowercase()
                .rsplit(['\\', '/'])
                .next()
                .is_some_and(|f| f == format!("{name}.dds"))
        })?;
        let dds = read_dds(dds.1)?;
        let rd = |at: usize| {
            u32::from_le_bytes([rec[at], rec[at + 1], rec[at + 2], rec[at + 3]]) as usize
        };
        let (gc_w, gc_h) = (rd(44), rd(48));
        let skel = chunks.get(i)?;
        let want_cmpr = rec[32] == CMPR;
        // Steam's own texture only when it is at least the GC size and has a
        // lossless route (DXT -> CMPR, anything -> RGB5A3); else Steam's GC one.
        if dds.w < gc_w || dds.h < gc_h || (want_cmpr && dds.kind == Kind::Rgba) {
            out.extend_from_slice(skel);
            continue;
        }
        let fmt = if want_cmpr { CMPR } else { RGB5A3 };
        let full = (dds.w.max(dds.h).trailing_zeros() + 1) as usize;
        let levels = if dds.levels.len() >= full { full } else { 1 };
        let mut data = Vec::new();
        for (l, level) in dds.levels.iter().take(levels).enumerate() {
            let (w, h) = ((dds.w >> l).max(1), (dds.h >> l).max(1));
            data.extend(match (fmt, &dds.kind) {
                (CMPR, Kind::Dxt1) => dxt1_to_cmpr(level, w, h),
                (CMPR, _) => dxt1_to_cmpr(&dxt5_colour_blocks(level), w, h),
                (_, Kind::Dxt1) => rgba_to_rgb5a3(&dxt1_rgba(level, w, h), w, h),
                (_, Kind::Dxt5) => rgba_to_rgb5a3(&decode_dxt5(level, w, h), w, h),
                (_, Kind::Rgba) => rgba_to_rgb5a3(level, w, h),
            });
        }
        let mip = u8::from(levels > 1);
        out[entry + 30] = mip;
        out[entry + 31] = fmt;
        out[entry + 33] = dims_code(dds.w) << 4 | dims_code(dds.h);
        out.extend_from_slice(b"GVRT");
        out.extend_from_slice(&(data.len() as u32 + 8).to_le_bytes());
        out.extend_from_slice(&[0, 0, mip, fmt]);
        out.extend_from_slice(&(dds.w as u16).to_be_bytes());
        out.extend_from_slice(&(dds.h as u16).to_be_bytes());
        out.extend(data);
    }
    // The game's archive loader (0x8001a190) reads the .prs to 0x811ffe60
    // minus its size rounded to 0x800 and unpacks it upward from 0x80fffe60:
    // one 2 MB scratch. A literal-only stream is longer than its output, so
    // the unpack never overtakes unread input; it fits while the rounded
    // .prs is within 2 MB. Bigger: keep Steam's GC-format archive.
    // shortcut: limit read from 0x8001a190 only; other archive loaders' scratch sizes unverified.
    let prs = prs_literal(&out);
    (prs.len().div_ceil(0x800) * 0x800 <= SCRATCH).then_some(prs)
}

/// DXT1 blocks to RGBA8 (only when the `.inf` asks for a non-CMPR format).
fn dxt1_rgba(blocks: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    for (bi, b) in blocks.chunks(8).enumerate() {
        let (bx, by) = ((bi % w.div_ceil(4)) * 4, (bi / w.div_ceil(4)) * 4);
        let (c0, c1) = (
            u16::from_le_bytes([b[0], b[1]]),
            u16::from_le_bytes([b[2], b[3]]),
        );
        let (p0, p1) = (rgb565(c0), rgb565(c1));
        let m = |k: [u16; 2], d: u16| -> [u8; 4] {
            let f = |i: usize| ((u16::from(p0[i]) * k[0] + u16::from(p1[i]) * k[1]) / d) as u8;
            [f(0), f(1), f(2), 255]
        };
        let pal = if c0 > c1 {
            [m([1, 0], 1), m([0, 1], 1), m([2, 1], 3), m([1, 2], 3)]
        } else {
            [m([1, 0], 1), m([0, 1], 1), m([1, 1], 2), [0; 4]]
        };
        let bits = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
        for i in 0..16 {
            let (x, y) = (bx + i % 4, by + i / 4);
            if x < w && y < h {
                let o = (y * w + x) * 4;
                out[o..o + 4].copy_from_slice(&pal[((bits >> (2 * i)) & 3) as usize]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dxt1_block_becomes_cmpr_tile_order_and_bits() {
        // Synthetic 8x8 DXT1: four blocks, colours LE, index byte 0b11_10_01_00.
        let mut b = Vec::new();
        for k in 0..4u8 {
            b.extend_from_slice(&[k, 0x12, 0x34, 0x56, 0b1110_0100, 0, 0, 0]);
        }
        let c = dxt1_to_cmpr(&b, 8, 8);
        assert_eq!(c.len(), 32);
        assert_eq!(&c[..8], &[0x12, 0, 0x56, 0x34, 0b0001_1011, 0, 0, 0]);
        assert_eq!(c[8 + 1], 1, "second block of the tile is block (0,1)");
    }

    #[test]
    fn dxt5_colour_blocks_stay_in_four_colour_mode() {
        // Synthetic DXT5 blocks: alpha half ignored; c0 < c1 gets swapped.
        let mut b = vec![0u8; 8];
        b.extend_from_slice(&[0x01, 0x00, 0x02, 0x00, 0b1110_0100, 0, 0, 0]);
        let c = dxt5_colour_blocks(&b);
        assert_eq!(&c[..4], &[0x02, 0x00, 0x01, 0x00]);
        assert_eq!(c[4], 0b1011_0001, "indices 0<->1 and 2<->3");
    }

    #[test]
    fn prs_literal_round_trips_through_the_reader() {
        let data: Vec<u8> = (0..1000u32).map(|i| (i * 7) as u8).collect();
        assert_eq!(decompress_prs(&prs_literal(&data), 4096).unwrap(), data);
    }

    #[test]
    fn rgb5a3_picks_opaque_and_translucent_forms() {
        let t = rgba_to_rgb5a3(&[255, 0, 0, 255, 0, 0, 255, 0x80], 2, 1);
        assert_eq!(&t[..2], &[0xFC, 0x00]);
        assert_eq!(&t[2..4], &[0x40, 0x0F]);
    }

    /// Local only: every installed Steam pak converts on its Steam .prs skeleton.
    #[test]
    #[ignore]
    fn installed_steam_paks_convert() {
        let gd = std::path::Path::new(
            "C:/Program Files (x86)/Steam/steamapps/common/Sonic Adventure 2/resource/gd_PC",
        );
        let (mut ok, mut skipped) = (0, Vec::new());
        for e in std::fs::read_dir(gd.join("PRS")).unwrap().flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let stem = name.trim_end_matches(".pak");
            let Ok(skel) = std::fs::read(gd.join(format!("{stem}.prs"))) else {
                skipped.push(name);
                continue;
            };
            let pak = std::fs::read(e.path()).unwrap();
            let prs = pak_to_prs(&pak, &skel).unwrap_or_else(|| panic!("{name}"));
            let gvm = decompress_prs(&prs, 256 << 20).unwrap();
            assert_eq!(&gvm[..4], b"GVMH", "{name}");
            ok += 1;
        }
        eprintln!("converted {ok}; no Steam .prs beside: {skipped:?}");
        assert!(ok > 100);
    }
}
