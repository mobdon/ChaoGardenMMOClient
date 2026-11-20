//! Steam Data_DLL CartCourseModels, using CART_DLL.ini (85 entries) and
//! SAMT sonic/cart/course.h (108-byte CART_COURSE_MODEL on 32-bit PC).
use crate::{manifest::sha256, pe::read_pe, Error, Result};

#[derive(Debug)]
pub struct CoursePiece {
    /// File offsets of the native object headers; no relocation is performed.
    pub display: Option<usize>,
    pub collision: Option<usize>,
    pub move_offset: [f32; 3],
    pub move_angle: i32,
}

pub struct CourseTable {
    pub source_offset: usize,
    pub source_sha256: String,
    pub pieces: Vec<CoursePiece>,
}

pub fn read_course_table(dll: &[u8]) -> Result<CourseTable> {
    let pe = read_pe(dll)?;
    let export = pe
        .exports
        .iter()
        .find(|e| e.name == "CartCourseModels" && e.forwarded_to.is_none())
        .ok_or_else(|| Error::invalid("missing CartCourseModels data export"))?;
    const SIZE: usize = 85 * 108;
    let at = pe.rva_to_offset(export.rva, SIZE)?;
    let reader = crate::model::Reader { data: dll };
    let source = reader.span(at, SIZE)?;
    let mut pieces = Vec::with_capacity(85);
    for i in 0..85 {
        let row = at + i * 108;
        let object = |field| -> Result<Option<usize>> {
            let va = reader.u32(field)?;
            if va == 0 {
                return Ok(None);
            }
            let offset = pe.va_to_offset(va, 52)?;
            reader.span(offset, 52)?;
            Ok(Some(offset))
        };
        pieces.push(CoursePiece {
            display: object(row)?,
            collision: object(row + 4)?,
            move_offset: [
                reader.f32(row + 8)?,
                reader.f32(row + 12)?,
                reader.f32(row + 16)?,
            ],
            move_angle: reader.u32(row + 20)? as i32,
        });
    }
    Ok(CourseTable {
        source_offset: at,
        source_sha256: sha256(source),
        pieces,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires installed Steam SA2"]
    fn installed_course_table() {
        let root = std::path::PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap());
        let dll = std::fs::read(root.join("resource/gd_PC/DLL/Win32/Data_DLL.dll")).unwrap();
        let t = read_course_table(&dll).unwrap();
        assert_eq!(
            t.source_sha256,
            "20d6b1e4b01a8781cf2c5acb4dfdeb0c44d7c8a74beaa5ba957410a0f1313ab3"
        );
        assert_eq!(t.pieces.iter().filter(|p| p.display.is_some()).count(), 81);
        assert_eq!(
            t.pieces.iter().filter(|p| p.collision.is_some()).count(),
            81
        );
        for p in &t.pieces[1..4] {
            assert_eq!(p.move_offset, [0.0, 0.0, -400.0]);
            assert_eq!(p.move_angle, 0);
        }
    }
}
/// Borrow the root Basic attachment's native position bank (little-endian XYZ
/// binary32 triples). Child objects, node transforms and polygons are separate.
pub fn root_collision_positions<'a>(
    dll: &'a [u8],
    pe: &crate::pe::PeImage,
    piece: &CoursePiece,
) -> Result<Option<&'a [[u8; 12]]>> {
    let Some(root) = piece.collision else {
        return Ok(None);
    };
    let r = crate::model::Reader { data: dll };
    r.span(root, 52)?;
    let attachment = r.u32(root + 4)?;
    if attachment == 0 {
        return Ok(None);
    }
    let at = pe.va_to_offset(attachment, 40)?;
    r.span(at, 40)?;
    let count = r.u32(at + 8)? as usize;
    if count > 262_144 {
        return Err(Error::invalid("kart Basic vertex count exceeds limit"));
    }
    if count == 0 {
        return Ok(Some(&[]));
    }
    let size = count * 12;
    let first = pe.va_to_offset(r.u32(at)?, size)?;
    Ok(Some(r.span(first, size)?.as_chunks::<12>().0))
}

#[cfg(test)]
mod collision_reference {
    use super::*;

    #[test]
    #[ignore = "requires Steam DLL and independent SA Tools kart models"]
    fn compare_root_vertex_banks() {
        let root = std::path::PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap());
        let models =
            std::path::PathBuf::from(std::env::var_os("SA2_KART_MODEL_REFERENCE").unwrap());
        let dll = std::fs::read(root.join("resource/gd_PC/DLL/Win32/Data_DLL.dll")).unwrap();
        let pe = read_pe(&dll).unwrap();
        let table = read_course_table(&dll).unwrap();
        let mut compared = 0;
        let mut normalized_zeros = 0;
        let mut strip_count = 0;
        for (i, piece) in table.pieces.iter().enumerate() {
            let path = models.join(format!("{i:03}.sa1mdl"));
            let Some(bank) = root_collision_positions(&dll, &pe, piece).unwrap() else {
                assert!(!path.exists(), "missing native attachment for piece {i}");
                continue;
            };
            let bytes = std::fs::read(path).unwrap();
            let r = crate::model::Reader { data: &bytes };
            let root = r.u32(8).unwrap() as usize;
            let attach = r.u32(root + 4).unwrap() as usize;
            let first = r.u32(attach).unwrap() as usize;
            let count = r.u32(attach + 8).unwrap() as usize;
            let reference = r.span(first, count * 12).unwrap();
            assert_eq!(bank.len(), count);
            for (a, b) in bank
                .as_flattened()
                .chunks_exact(4)
                .zip(reference.chunks_exact(4))
            {
                if a != b {
                    assert_eq!(
                        (a, b),
                        (&0x80000000u32.to_le_bytes()[..], &0u32.to_le_bytes()[..]),
                        "piece {i}"
                    );
                    normalized_zeros += 1;
                }
            }
            let mesh = r.u32(attach + 12).unwrap() as usize;
            let mut cursor = r.u32(mesh + 4).unwrap() as usize;
            let strips = root_collision_strips(&dll, &pe, piece).unwrap().unwrap();
            assert_eq!(strips.len(), usize::from(r.u16(mesh + 2).unwrap()));
            strip_count += strips.len();
            for strip in strips {
                let header = r.u16(cursor).unwrap();
                assert_eq!(strip.reversed, header & 0x8000 != 0);
                assert_eq!(strip.indices.len(), usize::from(header & 0x7fff));
                cursor += 2;
                for index in strip.indices {
                    assert_eq!(index, r.u16(cursor).unwrap());
                    cursor += 2;
                }
            }
            compared += 1;
        }
        assert_eq!(compared, 81);
        assert_eq!(normalized_zeros, 191);
        assert_eq!(strip_count, 428);
    }
}
/// Source triangle strips for a Steam kart root collision attachment. Each
/// header retains its winding bit; indices address root_collision_positions.
pub struct CollisionStrip {
    pub reversed: bool,
    pub indices: Vec<u16>,
}

pub fn root_collision_strips(
    dll: &[u8],
    pe: &crate::pe::PeImage,
    piece: &CoursePiece,
) -> Result<Option<Vec<CollisionStrip>>> {
    let Some(vertices) = root_collision_positions(dll, pe, piece)? else {
        return Ok(None);
    };
    let r = crate::model::Reader { data: dll };
    let at = pe.va_to_offset(
        r.u32(
            piece
                .collision
                .ok_or_else(|| Error::invalid("missing kart root"))?
                + 4,
        )?,
        40,
    )?;
    if r.u16(at + 20)? != 1 {
        return Err(Error::invalid("unexpected kart meshset count"));
    }
    let mesh = pe.va_to_offset(r.u32(at + 12)?, 24)?;
    if r.u16(mesh)? >> 14 != 3 {
        return Err(Error::invalid("expected kart collision triangle strips"));
    }
    let mut cursor = r.u32(mesh + 4)?;
    let mut strips = Vec::new();
    let mut total = 0usize;
    for _ in 0..r.u16(mesh + 2)? {
        let header = r.u16(pe.va_to_offset(cursor, 2)?)?;
        let count = usize::from(header & 0x7fff);
        total += count;
        if count < 3 || total > 2_000_000 {
            return Err(Error::invalid("invalid kart strip size"));
        }
        let bytes = 2 + 2 * count;
        let offset = pe.va_to_offset(cursor, bytes)?;
        let mut indices = Vec::with_capacity(count);
        for j in 0..count {
            let index = r.u16(offset + 2 + j * 2)?;
            if usize::from(index) >= vertices.len() {
                return Err(Error::invalid("kart collision vertex index out of bounds"));
            }
            indices.push(index);
        }
        strips.push(CollisionStrip {
            reversed: header & 0x8000 != 0,
            indices,
        });
        cursor = cursor
            .checked_add(bytes as u32)
            .ok_or_else(|| Error::invalid("kart strip address overflow"))?;
    }
    Ok(Some(strips))
}

/// Course-relative pose. The caller supplies any stage/world origin.
/// Heading uses one unsigned 16-bit revolution, as in the native piece data.
#[derive(Debug, Clone, Copy, Default)]
pub struct CoursePose {
    pub position: [f64; 3],
    pub heading: u16,
}

impl CoursePose {
    pub fn transform(self, local: [f32; 3]) -> [f64; 3] {
        let theta = f64::from(self.heading) * (std::f64::consts::TAU / 65536.0);
        let (s, c) = theta.sin_cos();
        let [x, y, z] = local.map(f64::from);
        [
            self.position[0] + c * x + s * z,
            self.position[1] + y,
            self.position[2] - s * x + c * z,
        ]
    }
}

/// One origin per ordered piece, followed by the terminal endpoint. Uses
/// mathematical sine/cosine; original engine rounding is not established.
pub fn course_layout(
    table: &CourseTable,
    ids: &[u8],
    start: CoursePose,
) -> Result<Vec<CoursePose>> {
    let mut poses = vec![start];
    let mut pose = start;
    for &id in ids {
        let piece = table
            .pieces
            .get(usize::from(id))
            .ok_or_else(|| Error::invalid("unknown kart piece ID"))?;
        if !piece.move_offset.iter().all(|v| v.is_finite()) {
            return Err(Error::invalid("nonfinite kart piece displacement"));
        }
        pose = CoursePose {
            position: pose.transform(piece.move_offset),
            heading: pose.heading.wrapping_add(piece.move_angle as u16),
        };
        poses.push(pose);
    }
    Ok(poses)
}

#[cfg(test)]
mod layout_reference {
    use super::*;
    #[test]
    #[ignore = "requires Steam game and independently split course lists"]
    fn matches_story_set_neighborhoods() {
        let game = std::path::PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap())
            .join("resource/gd_PC");
        let courses =
            std::path::PathBuf::from(std::env::var_os("SA2_KART_COURSE_REFERENCE").unwrap());
        let dll = std::fs::read(game.join("DLL/Win32/Data_DLL.dll")).unwrap();
        let pe = read_pe(&dll).unwrap();
        let table = read_course_table(&dll).unwrap();
        for (name, ch, count, lo, hi) in [
            ("101", "Tails", 321, 105., 111.),
            ("280", "Rouge", 369, 169., 175.),
        ] {
            let text = std::fs::read_to_string(courses.join(format!("Route {name}.txt"))).unwrap();
            let ids: Vec<u8> = text
                .trim_start_matches('\u{feff}')
                .split_whitespace()
                .map(|v| v.parse().unwrap())
                .collect();
            let poses = course_layout(&table, &ids, CoursePose::default()).unwrap();
            assert_eq!(poses.len(), count + 1);
            let mut triangles = 0;
            for (&id, &pose) in ids.iter().zip(&poses) {
                visit_piece_triangles(&dll, &pe, &table.pieces[usize::from(id)], pose, |t| {
                    assert!(t.points.iter().flatten().all(|v| v.is_finite()));
                    triangles += 1;
                })
                .unwrap();
            }
            assert_eq!(triangles, if name == "101" { 13643 } else { 18439 });
            let placements = crate::set::parse_set(
                &std::fs::read(game.join(format!("setCart{ch}.bin"))).unwrap(),
            )
            .unwrap();
            let mut nearest: Vec<f64> = placements
                .iter()
                .map(|p| {
                    poses
                        .iter()
                        .map(|q| {
                            (0..3)
                                .map(|k| (q.position[k] - f64::from(p.position[k])).powi(2))
                                .sum::<f64>()
                                .sqrt()
                        })
                        .fold(f64::INFINITY, f64::min)
                })
                .collect();
            nearest.sort_by(f64::total_cmp);
            let n = nearest.len();
            let median = (nearest[(n - 1) / 2] + nearest[n / 2]) * 0.5;
            assert!((lo..hi).contains(&median), "route {name}: {median}");
        }
    }
}

/// Positioned triangles of one course occurrence. Keep degenerate strip
/// triangles in their source slots; callers can discard them for queries.
pub struct PositionedKartTriangle {
    pub strip: usize,
    pub triangle: usize,
    pub vertex_indices: [u16; 3],
    pub points: [[f64; 3]; 3],
}

pub fn visit_piece_triangles(
    dll: &[u8],
    pe: &crate::pe::PeImage,
    piece: &CoursePiece,
    pose: CoursePose,
    mut visit: impl FnMut(PositionedKartTriangle),
) -> Result<()> {
    let root = piece
        .collision
        .ok_or_else(|| Error::invalid("missing kart collision root"))?;
    let r = crate::model::Reader { data: dll };
    r.span(root, 52)?;
    if r.u32(root + 44)? != 0
        || r.u32(root + 48)? != 0
        || (0..3).any(|i| {
            r.u32(root + 20 + 4 * i).ok() != Some(0) || r.f32(root + 32 + 4 * i).ok() != Some(1.0)
        })
    {
        return Err(Error::invalid(
            "unsupported kart collision node transform or hierarchy",
        ));
    }
    let offset = if r.u32(root)? & 1 != 0 {
        [0.; 3]
    } else {
        r.vec3(root + 8)?
    };
    let bank = root_collision_positions(dll, pe, piece)?
        .ok_or_else(|| Error::invalid("missing kart collision positions"))?;
    let points: Vec<_> = bank
        .iter()
        .map(|v| {
            pose.transform(std::array::from_fn(|i| {
                f32::from_le_bytes(v[4 * i..4 * i + 4].try_into().unwrap()) + offset[i]
            }))
        })
        .collect();
    for (strip, s) in root_collision_strips(dll, pe, piece)?
        .ok_or_else(|| Error::invalid("missing kart strips"))?
        .into_iter()
        .enumerate()
    {
        for (triangle, w) in s.indices.windows(3).enumerate() {
            let mut idx = [w[0], w[1], w[2]];
            if (triangle % 2 == 1) ^ s.reversed {
                idx.swap(0, 1);
            }
            visit(PositionedKartTriangle {
                strip,
                triangle,
                vertex_indices: idx,
                points: idx.map(|i| points[usize::from(i)]),
            });
        }
    }
    Ok(())
}

/// Geometric intersection in the route layout's native units, independent of kart dynamics.
#[derive(Debug, serde::Serialize)]
pub struct KartHit {
    pub occurrence: usize,
    pub piece_id: u8,
    pub strip: usize,
    pub triangle: usize,
    pub vertex_indices: [u16; 3],
    pub distance: f64,
    pub position: [f64; 3],
    pub normal: [f64; 3],
    pub barycentric: [f64; 3],
    pub front_face: bool,
}
#[derive(Debug, serde::Serialize)]
pub struct Course<'a> {
    pub name: &'static str,
    pub descriptor_rva: u32,
    pub piece_ids: &'a [u8],
    pub piece_ids_sha256: String,
}

pub fn courses(exe: &[u8]) -> Result<Vec<Course<'_>>> {
    let pe = crate::object_table::verify_image(exe)?;
    let r = crate::model::Reader { data: exe };
    [
        ("101", 0xaf82bc, 321),
        ("280", 0xaf82c4, 369),
        ("beginner", 0xaf82cc, 88),
        ("standard", 0xaf82d4, 112),
        ("expert", 0xaf82dc, 123),
    ]
    .into_iter()
    .map(|(name, descriptor_rva, n)| {
        let piece_ids = read_piece_ids(&r, &pe, descriptor_rva, n)?;
        Ok(Course {
            name,
            descriptor_rva,
            piece_ids,
            piece_ids_sha256: sha256(piece_ids),
        })
    })
    .collect()
}

pub(crate) fn read_piece_ids<'a>(
    r: &crate::model::Reader<'a>,
    pe: &crate::pe::PeImage,
    rva: u32,
    n: usize,
) -> Result<&'a [u8]> {
    let at = pe.rva_to_offset(rva, 8)?;
    if r.u32(at + 4)? as usize != n {
        return Err(Error::invalid("unknown kart course length"));
    }
    let ptr = pe.va_to_offset(r.u32(at)?, n)?;
    let ids = r.span(ptr, n)?;
    if ids.iter().any(|&v| v >= 85) {
        return Err(Error::invalid("course piece outside native table"));
    }
    Ok(ids)
}

pub fn story_raycast(
    exe: &[u8],
    dll: &[u8],
    route: u16,
    origin: [f64; 3],
    direction: [f64; 3],
    limit: f64,
) -> Result<Option<KartHit>> {
    let name = match route {
        101 => "101",
        280 => "280",
        _ => return Err(Error::invalid("unknown story kart route")),
    };
    Ok(course_raycast(exe, dll, name, origin, direction, limit)?.hit)
}

pub struct CourseQuery {
    pub hit: Option<KartHit>,
    pub without_collision: Vec<(usize, u8)>,
}

pub fn course_raycast(
    exe: &[u8],
    dll: &[u8],
    name: &str,
    origin: [f64; 3],
    direction: [f64; 3],
    limit: f64,
) -> Result<CourseQuery> {
    if !origin.iter().all(|v| v.is_finite()) || !limit.is_finite() || limit <= 0. {
        return Err(Error::invalid("invalid kart ray origin or distance"));
    }
    let direction = crate::collision::unit_direction(direction)?;
    let course = courses(exe)?
        .into_iter()
        .find(|c| c.name == name)
        .ok_or_else(|| Error::invalid("unknown kart course"))?;
    if sha256(dll) != crate::board_motion::PC_DLL_SHA256 {
        return Err(Error::invalid("unsupported kart DLL revision"));
    }
    let ids = course.piece_ids;
    let pe = read_pe(dll)?;
    let table = read_course_table(dll)?;
    let poses = course_layout(&table, ids, CoursePose::default())?;
    let mut without_collision = Vec::new();
    let mut nearest = None;
    let mut closest = limit;
    for (occurrence, (&piece_id, pose)) in ids.iter().zip(poses).enumerate() {
        if table.pieces[usize::from(piece_id)].collision.is_none() {
            without_collision.push((occurrence, piece_id));
            continue;
        }
        let mut result = Ok(());
        visit_piece_triangles(dll, &pe, &table.pieces[usize::from(piece_id)], pose, |t| {
            if result.is_err() {
                return;
            }
            match crate::collision::triangle_hit(origin, direction, closest, t.points) {
                Err(e) => result = Err(e),
                Ok(Some((distance, position, normal, barycentric, front_face)))
                    if nearest.is_none() || distance < closest =>
                {
                    closest = distance;
                    nearest = Some(KartHit {
                        occurrence,
                        piece_id,
                        strip: t.strip,
                        triangle: t.triangle,
                        vertex_indices: t.vertex_indices,
                        distance,
                        position,
                        normal,
                        barycentric,
                        front_face,
                    });
                }
                _ => {}
            }
        })?;
        result?;
    }
    Ok(CourseQuery {
        hit: nearest,
        without_collision,
    })
}

/// One course texture slot used by the placed display pieces.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseTexture {
    pub slot: u16,
    pub name: String,
    pub entry: String,
    pub offset: usize,
    pub length: usize,
    pub width: u32,
    pub height: u32,
    pub dds_sha256: String,
    pub rgba_sha256: String,
    /// Decoded texels whose alpha is below 255; the pixels are not altered.
    pub translucent_texels: usize,
}

/// Native GC parameters of one display mesh, in model mesh order.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseMesh {
    /// Mesh from the attachment's translucent list (both lists are kept).
    pub translucent_list: bool,
    /// Low half of the mesh's own texture parameter: a course texture slot.
    pub texture_slot: Option<u16>,
    /// High half of that parameter, retained unchanged.
    pub texture_flags: Option<u16>,
    /// Raw blend-alpha parameter word, when the mesh declares one.
    pub blend_alpha: Option<u32>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoursePieceMaterials {
    pub piece_id: u8,
    pub occurrences: usize,
    pub meshes: Vec<CourseMesh>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseMaterials {
    pub course: &'static str,
    /// Data_DLL `texlist_course` export and its slot names, in order.
    pub texture_list_rva: u32,
    pub texture_names: Vec<String>,
    /// CartD section-5 list bound by the course display callback (+0x684c).
    pub cart_display_list: u32,
    pub textures: Vec<CourseTexture>,
    pub pieces: Vec<CoursePieceMaterials>,
    pub meshes: usize,
    pub meshes_without_texture: usize,
}

const CART_D_SHA256: &str = "71826645bae2a4064cd5ad4402facaf4da6c5caee0be7e19894ed7ceb2c2ee6c";
/// CartD +0x684c loads section 5 +0xf9c34 at +0x6860 and passes it to the
/// imported routine 0x80116ad0 at +0x6868 before drawing course pieces.
const CART_COURSE_LIST: u32 = 0xf9c34;

fn cart_display_names(cart: &[u8]) -> Result<Vec<String>> {
    if sha256(cart) != CART_D_SHA256 {
        return Err(Error::invalid("unsupported CartD.rel revision"));
    }
    let rel = crate::city_rel::CityRel::read_identified(cart, 80)?;
    let invalid = || Error::invalid("unexpected CartD course display binding");
    let list = rel
        .reference(1, 0x6866)
        .filter(|r| (r.module, r.section, r.kind) == (80, 5, 4))
        .map(|r| r.addend)
        .ok_or_else(invalid)?;
    let call = rel.reference(1, 0x6868).ok_or_else(invalid)?;
    if list != CART_COURSE_LIST || (call.module, call.kind, call.addend) != (0, 10, 0x8011_6ad0) {
        return Err(invalid());
    }
    let data = rel.data(5)?;
    let ptr = |at: u32| -> Result<usize> {
        rel.local_pointer(5, at)?
            .filter(|(s, _)| *s == 5)
            .map(|(_, v)| v as usize)
            .ok_or_else(invalid)
    };
    let names = ptr(list)?;
    let count = data
        .get(list as usize + 4..list as usize + 8)
        .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
        .ok_or_else(invalid)?;
    if count > 4096 {
        return Err(invalid());
    }
    (0..count as usize)
        .map(|k| {
            let at = ptr((names + 12 * k) as u32)?;
            let name = std::ffi::CStr::from_bytes_until_nul(data.get(at..).ok_or_else(invalid)?)
                .map_err(|_| invalid())?;
            Ok(name.to_str().map_err(|_| invalid())?.trim_end().to_owned())
        })
        .collect()
}

fn dll_course_names(dll: &[u8], pe: &crate::pe::PeImage) -> Result<(u32, Vec<String>)> {
    let export = pe
        .exports
        .iter()
        .find(|e| e.name == "texlist_course" && e.forwarded_to.is_none())
        .ok_or_else(|| Error::invalid("missing texlist_course data export"))?;
    let r = crate::model::Reader { data: dll };
    let at = pe.rva_to_offset(export.rva, 8)?;
    let count = r.u32(at + 4)? as usize;
    if count > 4096 {
        return Err(Error::invalid("course texture list exceeds limit"));
    }
    let names = pe.va_to_offset(r.u32(at)?, count * 12)?;
    let list = (0..count)
        .map(|k| {
            let s = pe.va_to_offset(r.u32(names + 12 * k)?, 1)?;
            let name = std::ffi::CStr::from_bytes_until_nul(&dll[s..])
                .map_err(|_| Error::invalid("unterminated course texture name"))?;
            Ok(name
                .to_str()
                .map_err(|_| Error::invalid("non-ASCII course texture name"))?
                .trim_end()
                .to_owned())
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((export.rva, list))
}

/// Mesh parameters of one PC GC display object and its hierarchy: the
/// attachment keeps opaque/translucent mesh list pointers at +8/+12 and their
/// counts at +16/+18; each 16-byte mesh starts with its parameter list pointer
/// and count; each 8-byte parameter is a type word (4 blend alpha, 8 texture)
/// followed by its data word.
fn display_meshes(dll: &[u8], pe: &crate::pe::PeImage, root: usize) -> Result<Vec<CourseMesh>> {
    let r = crate::model::Reader { data: dll };
    let mut meshes = Vec::new();
    let mut stack = vec![root];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(node) = stack.pop() {
        if !seen.insert(node) || seen.len() > 4096 {
            return Err(Error::invalid("invalid kart display hierarchy"));
        }
        r.span(node, 52)?;
        for link in [48, 44] {
            let va = r.u32(node + link)?;
            if va != 0 {
                stack.push(pe.va_to_offset(va, 52)?);
            }
        }
        let attach = r.u32(node + 4)?;
        if attach == 0 {
            continue;
        }
        let a = pe.va_to_offset(attach, 36)?;
        for (list, count, translucent_list) in [(8, 16, false), (12, 18, true)] {
            let n = r.u16(a + count)? as i16;
            if n < 0 {
                return Err(Error::invalid("negative kart GC mesh count"));
            }
            if n == 0 {
                continue;
            }
            let base = pe.va_to_offset(r.u32(a + list)?, 16 * n as usize)?;
            for m in 0..n as usize {
                let params = r.u32(base + 16 * m + 4)? as usize;
                if params > 64 {
                    return Err(Error::invalid("kart GC parameter count exceeds limit"));
                }
                let mut mesh = CourseMesh {
                    translucent_list,
                    texture_slot: None,
                    texture_flags: None,
                    blend_alpha: None,
                };
                if params != 0 {
                    let p = pe.va_to_offset(r.u32(base + 16 * m)?, 8 * params)?;
                    for k in 0..params {
                        let value = r.u32(p + 8 * k + 4)?;
                        let repeated = match r.u32(p + 8 * k)? {
                            4 => mesh.blend_alpha.replace(value).is_some(),
                            8 => {
                                mesh.texture_flags = Some((value >> 16) as u16);
                                mesh.texture_slot.replace(value as u16).is_some()
                            }
                            _ => false,
                        };
                        if repeated {
                            return Err(Error::invalid("repeated kart GC mesh parameter"));
                        }
                    }
                }
                meshes.push(mesh);
            }
        }
    }
    Ok(meshes)
}

/// Native texture binding for a course's display pieces: the slot names the
/// CartD course display binds (checked equal to Data_DLL `texlist_course`),
/// each used slot resolved to its course.pak DDS, and every display mesh's
/// own texture and blend parameters. No renderer state is inferred.
pub fn course_materials(
    exe: &[u8],
    dll: &[u8],
    cart: &[u8],
    pak: &[u8],
    name: &str,
) -> Result<CourseMaterials> {
    if sha256(dll) != crate::board_motion::PC_DLL_SHA256 {
        return Err(Error::invalid("unsupported kart DLL revision"));
    }
    let course = courses(exe)?
        .into_iter()
        .find(|c| c.name == name)
        .ok_or_else(|| Error::invalid("unknown kart course"))?;
    let pe = read_pe(dll)?;
    let (texture_list_rva, texture_names) = dll_course_names(dll, &pe)?;
    if cart_display_names(cart)? != texture_names {
        return Err(Error::invalid(
            "CartD course display list differs from texlist_course",
        ));
    }
    let table = read_course_table(dll)?;
    let mut counts = std::collections::BTreeMap::new();
    for &id in course.piece_ids {
        *counts.entry(id).or_insert(0usize) += 1;
    }
    let mut pieces = Vec::new();
    let mut used = std::collections::BTreeSet::new();
    let (mut meshes, mut meshes_without_texture) = (0, 0);
    for (&piece_id, &occurrences) in &counts {
        let Some(root) = table.pieces[usize::from(piece_id)].display else {
            continue;
        };
        let list = display_meshes(dll, &pe, root)?;
        for mesh in &list {
            meshes += occurrences;
            match mesh.texture_slot {
                Some(slot) if usize::from(slot) < texture_names.len() => {
                    used.insert(slot);
                }
                Some(_) => return Err(Error::invalid("kart texture slot outside course list")),
                None => meshes_without_texture += occurrences,
            }
        }
        pieces.push(CoursePieceMaterials {
            piece_id,
            occurrences,
            meshes: list,
        });
    }
    let bank = crate::archive::read_pak(pak)?;
    let textures = used
        .into_iter()
        .map(|slot| {
            let name = &texture_names[usize::from(slot)];
            let member = format!("course\\{name}.dds");
            let entry = bank
                .iter()
                .find(|e| e.name.eq_ignore_ascii_case(&member))
                .ok_or_else(|| Error::invalid(format!("course.pak lacks {member}")))?;
            let image = crate::texture::decode_dds(entry.data)?;
            Ok(CourseTexture {
                slot,
                name: name.clone(),
                entry: entry.name.clone(),
                offset: entry.offset,
                length: entry.length,
                width: image.width,
                height: image.height,
                dds_sha256: sha256(entry.data),
                rgba_sha256: sha256(&image.rgba),
                translucent_texels: image
                    .rgba
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|p| p[3] < 255)
                    .count(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(CourseMaterials {
        course: course.name,
        texture_list_rva,
        texture_names,
        cart_display_list: CART_COURSE_LIST,
        textures,
        pieces,
        meshes,
        meshes_without_texture,
    })
}

/// One roadside slot of a course row (row +0x64, 16 bytes each): a
/// piece-local position and a signed BAMS yaw for the shared billboard model.
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BillboardSlot {
    pub position: [f32; 3],
    pub yaw: i32,
}

/// One placed occurrence after CartD course setup (+0x71b8..+0x72a0).
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BillboardOccurrence {
    pub occurrence: usize,
    pub piece_id: u8,
    /// Row slot count (row +0x68); only the first four can ever be shown.
    pub slot_count: u32,
    /// Live entry words +0x20..+0x2c: 0 hides the slot, n selects picture
    /// n - 1. None means setup leaves the words unwritten (the last 20
    /// occurrences); their contents come from earlier memory, not this data.
    pub pictures: Option<[i32; 4]>,
    pub slots: Vec<BillboardSlot>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseBillboards {
    pub course: &'static str,
    /// CartD section 5 model drawn for every shown slot (helper +0xc2f8).
    pub model: u32,
    /// Course texture slot written into the model per picture (section 5
    /// +0x2edbf8), with its name from the course list.
    pub picture_textures: Vec<(u16, String)>,
    /// Squared distance from the active camera position below which the course display draws a
    /// piece and its shown slots: [engine byte 0x801cbd88+0x20 set, clear].
    pub draw_distance_squared: [f32; 2],
    pub rand_calls: usize,
    pub occurrences: Vec<BillboardOccurrence>,
}

fn row_slots(dll: &[u8], pe: &crate::pe::PeImage, id: u8) -> Result<(u32, Vec<BillboardSlot>)> {
    let export = pe
        .exports
        .iter()
        .find(|e| e.name == "CartCourseModels" && e.forwarded_to.is_none())
        .ok_or_else(|| Error::invalid("missing CartCourseModels data export"))?;
    let r = crate::model::Reader { data: dll };
    let row = pe.rva_to_offset(export.rva, 85 * 108)? + 108 * usize::from(id);
    let count = r.u32(row + 0x68)?;
    if count > 64 {
        return Err(Error::invalid("kart billboard slot count exceeds limit"));
    }
    if count == 0 {
        return Ok((0, vec![]));
    }
    let at = pe.va_to_offset(r.u32(row + 0x64)?, 16 * count as usize)?;
    let slots = (0..count as usize)
        .map(|k| {
            Ok(BillboardSlot {
                position: r.vec3(at + 16 * k)?,
                yaw: r.u32(at + 16 * k + 12)? as i32,
            })
        })
        .collect::<Result<_>>()?;
    Ok((count, slots))
}

/// CartD course setup's picture choice for one slot, from two engine rand()
/// results: u = (float)rand * (1/32768); a slot shows only when u > 0.5, and
/// then picture = trunc((float)rand2 * (1/32768) * 4 + 1), all in binary32.
fn picture(rand: &mut impl Iterator<Item = i32>) -> Result<i32> {
    let mut next = || {
        rand.next()
            .map(|v| v as f32 * (1. / 32768.))
            .ok_or_else(|| Error::invalid("missing kart rand result"))
    };
    if next()? > 0.5 {
        Ok((next()? * 4. + 1.) as i32)
    } else {
        Ok(0)
    }
}

/// Billboard pictures, slots and draw rule for a course, from the Steam
/// Data_DLL rows and CartD. `rand` supplies the engine rand() returns in
/// native call order; missing or unused results are rejected.
pub fn course_billboards(
    exe: &[u8],
    dll: &[u8],
    cart: &[u8],
    name: &str,
    rand: &[i32],
) -> Result<CourseBillboards> {
    if sha256(dll) != crate::board_motion::PC_DLL_SHA256 {
        return Err(Error::invalid("unsupported kart DLL revision"));
    }
    let course = courses(exe)?
        .into_iter()
        .find(|c| c.name == name)
        .ok_or_else(|| Error::invalid("unknown kart course"))?;
    let texture_names = cart_display_names(cart)?;
    let rel = crate::city_rel::CityRel::read_identified(cart, 80)?;
    let invalid = || Error::invalid("unexpected CartD billboard code");
    let target = |site, section| {
        rel.reference(1, site)
            .filter(|r| (r.module, r.section, r.kind) == (80, section, 4))
            .map(|r| r.addend)
            .ok_or_else(invalid)
    };
    let model = target(0xc322, 5)?;
    let variants = target(0xc336, 5)?;
    let near = [target(0x69fe, 4)?, target(0x6a1a, 4)?];
    if (model, variants, target(0xc366, 5)?) != (0x7e6c, 0x2edbf8, CART_COURSE_LIST)
        || near != [0x2ac, 0x2b0]
    {
        return Err(invalid());
    }
    let word = |section, at: u32| -> Result<u32> {
        rel.data(section)?
            .get(at as usize..at as usize + 4)
            .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
            .ok_or_else(invalid)
    };
    let picture_textures = (0..4)
        .map(|k| {
            let slot = word(5, variants + 4 * k)?;
            let name = texture_names.get(slot as usize).ok_or_else(invalid)?;
            Ok((slot as u16, name.clone()))
        })
        .collect::<Result<Vec<_>>>()?;
    let draw_distance_squared = [
        f32::from_bits(word(4, near[0])?),
        f32::from_bits(word(4, near[1])?),
    ];
    let pe = read_pe(dll)?;
    let ids = course.piece_ids;
    let mut values = rand.iter().copied();
    let mut occurrences = Vec::with_capacity(ids.len());
    for (occurrence, &piece_id) in ids.iter().enumerate() {
        let (slot_count, slots) = row_slots(dll, &pe, piece_id)?;
        let pictures = if occurrence + 20 < ids.len() {
            let mut p = [0; 4];
            for (k, value) in p.iter_mut().enumerate() {
                if (k as u32) < slot_count {
                    *value = picture(&mut values)?;
                }
            }
            Some(p)
        } else {
            None
        };
        occurrences.push(BillboardOccurrence {
            occurrence,
            piece_id,
            slot_count,
            pictures,
            slots,
        });
    }
    let rand_calls = rand.len() - values.len();
    if values.next().is_some() {
        return Err(Error::invalid("unused kart rand results"));
    }
    Ok(CourseBillboards {
        course: course.name,
        model,
        picture_textures,
        draw_distance_squared,
        rand_calls,
        occurrences,
    })
}

#[cfg(test)]
mod billboard_choice {
    use super::picture;

    #[test]
    fn native_threshold_and_truncation() {
        let pick = |v: &[i32]| picture(&mut v.iter().copied()).unwrap();
        // u must be strictly above 0.5; 16384/32768 is exactly 0.5.
        assert_eq!(pick(&[16384]), 0);
        assert_eq!(pick(&[16385, 0]), 1);
        assert_eq!(pick(&[32767, 8191]), 1);
        assert_eq!(pick(&[32767, 8192]), 2);
        assert_eq!(pick(&[32767, 32767]), 4);
        assert!(picture(&mut [20000].into_iter()).is_err());
    }
}

/// One live course entry after CartD course setup: the occurrence's piece,
/// its origin and its accumulated yaw word (entry +0, +4, +0x18).
#[derive(Debug, Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePlacement {
    pub piece_id: u8,
    pub position: [f32; 3],
    pub yaw: i32,
}

/// `a * c + b` rounded once to double and then to single, as the paired
/// multiply-add is evaluated.
fn madd(a: f32, c: f32, b: f32) -> f32 {
    (a as f64 * c as f64 + b as f64) as f32
}

/// CartD course setup (+0x72a4..+0x7368), using the GSNE8P main.dol
/// routines its imports resolve to. The first occurrence sits at section 4
/// +0x2e4 (0.0) with yaw 0. Each later one takes the previous occurrence's
/// row: njUnitMatrix, njRotateY(previous yaw) (MSL sinf/cosf through
/// `ninja_trig::nj_rotate_sin_cos`), njCalcVector of the row displacement
/// (PSMTXMultVecSR: two single products summed, then one fused add of the
/// third), njAddVector onto the previous origin (single adds), and the row's
/// turn added to the yaw word.
pub fn native_course_layout(table: &CourseTable, ids: &[u8]) -> Result<Vec<NativePlacement>> {
    let mut out: Vec<NativePlacement> = Vec::with_capacity(ids.len());
    for &piece_id in ids {
        if usize::from(piece_id) >= table.pieces.len() {
            return Err(Error::invalid("unknown kart piece ID"));
        }
        let placement = match out.last() {
            None => NativePlacement {
                piece_id,
                position: [0.0; 3],
                yaw: 0,
            },
            Some(prev) => {
                let row = &table.pieces[usize::from(prev.piece_id)];
                let (s, c) = crate::ninja_trig::nj_rotate_sin_cos(prev.yaw);
                let [x, y, z] = row.move_offset;
                // njRotateY on a unit matrix: rows [c 0 s] [0 1 0] [-s 0 c].
                let m = [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]];
                let v = m.map(|r| madd(r[2], z, r[0] * x + r[1] * y));
                NativePlacement {
                    piece_id,
                    position: std::array::from_fn(|k| prev.position[k] + v[k]),
                    yaw: prev.yaw.wrapping_add(row.move_angle),
                }
            }
        };
        out.push(placement);
    }
    Ok(out)
}

/// Native placements for one of the five installed courses.
pub fn native_course_placements(
    exe: &[u8],
    dll: &[u8],
    name: &str,
) -> Result<Vec<NativePlacement>> {
    if sha256(dll) != crate::board_motion::PC_DLL_SHA256 {
        return Err(Error::invalid("unsupported kart DLL revision"));
    }
    let course = courses(exe)?
        .into_iter()
        .find(|c| c.name == name)
        .ok_or_else(|| Error::invalid("unknown kart course"))?;
    native_course_layout(&read_course_table(dll)?, course.piece_ids)
}

#[cfg(test)]
mod native_layout {
    use super::*;

    #[test]
    fn straight_piece_moves_exactly_and_turn_accumulates() {
        let piece = |offset: [f32; 3], angle: i32| CoursePiece {
            display: None,
            collision: None,
            move_offset: offset,
            move_angle: angle,
        };
        let table = CourseTable {
            source_offset: 0,
            source_sha256: String::new(),
            pieces: vec![piece([0., 0., -400.], 0), piece([100., 0., -300.], 0x4000)],
        };
        let p = native_course_layout(&table, &[0, 1, 0]).unwrap();
        assert_eq!((p[0].position, p[0].yaw), ([0.; 3], 0));
        assert_eq!((p[1].position, p[1].yaw), ([0., 0., -400.], 0));
        assert_eq!(p[2].position, [100., 0., -700.]);
        assert_eq!(p[2].yaw, 0x4000);
        assert!(native_course_layout(&table, &[2]).is_err());
    }
}

/// Model-less pieces CartD gives no collision task (+0x6494 returns 1).
pub const UNTASKED_PIECES: std::ops::Range<u8> = 25..28;

/// The collision object a course piece's task builds (+0x65f0): a 0x38-byte
/// copy of the piece's Basic collision object whose flags lose bits 0-1 and
/// whose position and rotation become the occurrence's origin and (0, yaw, 0).
/// Attachment, scale, child and sibling stay as copied.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PieceCollision {
    pub occurrence: usize,
    pub piece_id: u8,
    /// File offset of the native Basic attachment in Data_DLL.
    pub attachment: Option<usize>,
    pub flags: u32,
    pub position: [f32; 3],
    pub rotation: [i32; 3],
    pub scale: [f32; 3],
    pub child: u32,
    pub sibling: u32,
    /// Flags passed with the object to the dynamic land list (PC import
    /// 0x8002305c, GSNE8P 0x800232A4): 0x20000001, plus 0x10000000 when the
    /// attachment centre lies at least 15 units from the object origin
    /// (squared length >= 225.0, main.dol r2-0x7ac8) and a rotation word
    /// has bits in 0xFFF0.
    pub surface_flags: u32,
}

fn dynamic_land_flags(base: u32, center: [f32; 3], rotation: [i32; 3]) -> u32 {
    // lfs y; fmuls y*y; fmadds x*x + that; fmadds z*z + that; fcmpo with 225.
    let r = center[1] * center[1];
    let r = (center[0] as f64 * center[0] as f64 + r as f64) as f32;
    let r = (center[2] as f64 * center[2] as f64 + r as f64) as f32;
    if r >= 225.0 && (rotation[0] | rotation[1] | rotation[2]) & 0xfff0 != 0 {
        base | 0x1000_0000
    } else {
        base
    }
}

/// Every tasked occurrence of a course (setup +0x63bc: all pieces except
/// 25..=27) with the native collision object its task registers.
pub fn piece_collision_tasks(exe: &[u8], dll: &[u8], name: &str) -> Result<Vec<PieceCollision>> {
    let course = courses(exe)?
        .into_iter()
        .find(|c| c.name == name)
        .ok_or_else(|| Error::invalid("unknown kart course"))?;
    piece_collision_for_ids(dll, course.piece_ids)
}

/// `piece_collision_tasks` for an ordered piece list from the Steam Data_DLL.
pub fn piece_collision_for_ids(dll: &[u8], ids: &[u8]) -> Result<Vec<PieceCollision>> {
    if sha256(dll) != crate::board_motion::PC_DLL_SHA256 {
        return Err(Error::invalid("unsupported kart DLL revision"));
    }
    let pe = read_pe(dll)?;
    let table = read_course_table(dll)?;
    let placements = native_course_layout(&table, ids)?;
    let r = crate::model::Reader { data: dll };
    let mut out = Vec::new();
    for (occurrence, p) in placements.iter().enumerate() {
        if UNTASKED_PIECES.contains(&p.piece_id) {
            continue;
        }
        let root = table.pieces[usize::from(p.piece_id)]
            .collision
            .ok_or_else(|| Error::invalid("tasked kart piece without collision object"))?;
        r.span(root, 0x38)?;
        let attach_va = r.u32(root + 4)?;
        let attachment = if attach_va == 0 {
            None
        } else {
            Some(pe.va_to_offset(attach_va, 0x28)?)
        };
        let center = match attachment {
            Some(a) => r.vec3(a + 0x18)?,
            None => [0.0; 3],
        };
        let rotation = [0, p.yaw, 0];
        out.push(PieceCollision {
            occurrence,
            piece_id: p.piece_id,
            attachment,
            flags: r.u32(root)? & !3,
            position: p.position,
            rotation,
            scale: r.vec3(root + 0x20)?,
            child: r.u32(root + 0x2c)?,
            sibling: r.u32(root + 0x30)?,
            surface_flags: dynamic_land_flags(0x2000_0001, center, rotation),
        });
    }
    Ok(out)
}

/// One piece task's per-frame decision (+0x64b8..+0x65dc), given the
/// race mode word (section 6 +0x1a0), the live course count and, for the six
/// kart slots in order, the occurrence that owns each kart's current segment
/// (None for an empty slot). True keeps the collision registered.
pub fn piece_collision_active(
    occurrence: u16,
    count: i32,
    race_mode: i32,
    kart_owners: &[Option<i32>; 6],
) -> bool {
    let occ = i32::from(occurrence);
    for owner in kart_owners {
        let Some(owner) = *owner else {
            return false;
        };
        if (0..2).contains(&race_mode) && occ >= count - 30 {
            return true;
        }
        if occurrence < 5 || occ > count - 5 {
            return true;
        }
        if owner - 10 <= occ && occ <= owner + 4 {
            return true;
        }
    }
    false
}

/// The posed collision objects of every tasked occurrence as one static
/// collision view: each entry's root is the task's native object copy (its
/// flags, attachment, position, rotation, scale and links as built at
/// +0x65f0) and its surface flags are the dynamic land flags it registers
/// with. Entries keep zero bounds; all are Basic collision.
pub fn course_collision_stage(dll: &[u8], ids: &[u8]) -> Result<crate::stage::Stage> {
    let pieces = piece_collision_for_ids(dll, ids)?;
    let table = read_course_table(dll)?;
    let pe = read_pe(dll)?;
    let objects: Vec<_> = pieces
        .iter()
        .map(|p| crate::city_land::PosedObject {
            root: table.pieces[usize::from(p.piece_id)].collision.unwrap(),
            flags: p.flags,
            position: p.position,
            rotation: p.rotation,
            surface_flags: p.surface_flags,
        })
        .collect();
    crate::city_land::win32_posed_collision(dll, &pe, &objects)
}

#[cfg(test)]
mod collision_stage {
    use super::*;

    /// Each landtable root is exactly the task's object copy, in order.
    #[test]
    #[ignore = "requires installed Steam SA2"]
    fn roots_are_the_task_copies() {
        let root = std::path::PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap());
        let exe = std::fs::read(root.join("sonic2app.exe")).unwrap();
        let dll = std::fs::read(root.join("resource/gd_PC/DLL/Win32/Data_DLL.dll")).unwrap();
        for course in courses(&exe).unwrap() {
            let pieces = piece_collision_for_ids(&dll, course.piece_ids).unwrap();
            let stage = course_collision_stage(&dll, course.piece_ids).unwrap();
            assert_eq!(stage.entries.len(), pieces.len());
            for (entry, piece) in stage.entries.iter().zip(&pieces) {
                let node = &entry.nodes[0];
                assert_eq!(entry.surface_flags, piece.surface_flags);
                assert_eq!(node.flags, piece.flags);
                assert_eq!(
                    node.position.map(f32::to_bits),
                    piece.position.map(f32::to_bits)
                );
                assert_eq!(node.rotation_bams, piece.rotation);
                assert_eq!(node.scale.map(f32::to_bits), piece.scale.map(f32::to_bits));
                assert_eq!(node.attach_offset as usize, piece.attachment.unwrap() + 16);
            }
        }
    }
}

/// The race background CartD's bgExec task draws each frame (+0xdfc calls
/// the distant view +0xd00, then the sea +0x9ac). Both follow the active
/// camera's X/Z; their height comes from the course's lowest piece origin.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CourseBackground {
    pub course: &'static str,
    /// Lowest Y over occurrences 1.. (course setup +0x736c, from 100000.0).
    pub minimum_y: f32,
    /// Section 5 object drawn at (camera X, minimum_y + offset, camera Z).
    pub distant_object: u32,
    pub distant_height_offset: f32,
    pub distant_textures: Vec<String>,
    /// Section 5 object drawn at (camera X, minimum_y, camera Z), scaled.
    pub sea_object: u32,
    pub sea_scale: f32,
    pub sea_grid: u32,
    /// Game frames per grid row of colour scroll (section 5 +0x368c).
    pub sea_period: u32,
    /// The sea's texture slot is (frame / 2) % 20 into this list.
    pub sea_textures: Vec<String>,
}

struct SeaSource {
    vertex_list: u32,
    count: u32,
    period: u32,
}

fn rel_names(rel: &crate::city_rel::CityRel<'_>, list: u32) -> Result<Vec<String>> {
    let data = rel.data(5)?;
    let invalid = || Error::invalid("unexpected CartD texture list");
    let ptr = |at: u32| -> Result<usize> {
        rel.local_pointer(5, at)?
            .filter(|(s, _)| *s == 5)
            .map(|(_, v)| v as usize)
            .ok_or_else(invalid)
    };
    let names = ptr(list)?;
    let count = data
        .get(list as usize + 4..list as usize + 8)
        .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
        .filter(|&n| n <= 4096)
        .ok_or_else(invalid)?;
    (0..count as usize)
        .map(|k| {
            let at = ptr((names + 12 * k) as u32)?;
            let name = std::ffi::CStr::from_bytes_until_nul(data.get(at..).ok_or_else(invalid)?)
                .map_err(|_| invalid())?;
            Ok(name.to_str().map_err(|_| invalid())?.trim_end().to_owned())
        })
        .collect()
}

fn background_module(cart: &[u8]) -> Result<(crate::city_rel::CityRel<'_>, [u32; 8])> {
    if sha256(cart) != CART_D_SHA256 {
        return Err(Error::invalid("unsupported CartD.rel revision"));
    }
    let rel = crate::city_rel::CityRel::read_identified(cart, 80)?;
    let invalid = || Error::invalid("unexpected CartD background code");
    // (LO relocation site, section) for each value the background code uses.
    let mut v = [0u32; 8];
    for (k, (site, section)) in [
        (0xd5e, 5),  // distant height offset
        (0xd82, 5),  // distant object
        (0xd1a, 5),  // distant texture list
        (0x9ca, 5),  // sea object
        (0xa56, 5),  // sea period
        (0xc32, 4),  // sea scale
        (0xbaa, 5),  // sea texture list
        (0x7106, 4), // course minimum start value
    ]
    .into_iter()
    .enumerate()
    {
        v[k] = rel
            .reference(1, site)
            .filter(|r| (r.module, r.section, r.kind) == (80, section, 4))
            .map(|r| r.addend)
            .ok_or_else(invalid)?;
    }
    if v != [0x3690, 0x35c4, 0x3680, 0x23bc, 0x368c, 0x160, 0x2764, 0x2d0] {
        return Err(invalid());
    }
    Ok((rel, v))
}

fn be_word(data: &[u8], at: u32) -> Result<u32> {
    data.get(at as usize..at as usize + 4)
        .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
        .ok_or_else(|| Error::invalid("CartD background word outside section"))
}

fn sea_source(rel: &crate::city_rel::CityRel<'_>, v: &[u32; 8]) -> Result<SeaSource> {
    let invalid = || Error::invalid("unexpected CartD sea model");
    let ptr = |at: u32| -> Result<u32> {
        rel.local_pointer(5, at)?
            .filter(|(s, _)| *s == 5)
            .map(|(_, v)| v)
            .ok_or_else(invalid)
    };
    let data = rel.data(5)?;
    let attach = ptr(v[3] + 4)?;
    let vertex_list = ptr(attach)?;
    let count = be_word(data, vertex_list + 4)? >> 16;
    // The vertex chunk must hold count 16-byte position/colour vertices.
    if be_word(data, vertex_list)? & 0xff != 0x23 || count == 0 || count > 4096 {
        return Err(invalid());
    }
    be_word(data, vertex_list + 8 + 16 * count)?;
    Ok(SeaSource {
        vertex_list,
        count,
        period: be_word(data, v[4])?,
    })
}

/// CartD +0xc74: square root through frsqrte and three Newton steps in
/// double precision, rounded to single (the input is the vertex count).
fn cart_sqrt(x: f32) -> f32 {
    if x <= 0.0 {
        return x;
    }
    let x = x as f64;
    let mut y = 1.0 / x.sqrt();
    for _ in 0..3 {
        y = (0.5 * y) * (3.0 - x * (y * y));
    }
    (x * y) as f32
}

/// main.dol colour blend (PC import 0x80033248, GSNE8P 0x800334B0): each
/// ARGB byte becomes trunc(t * (b - a) + a) in single precision with one
/// fused add; a result outside 0..=255 becomes 255.
pub fn blend_argb(a: u32, b: u32, t: f32) -> u32 {
    (0..4).fold(0, |out, k| {
        let shift = 24 - 8 * k;
        let (x, y) = (((a >> shift) & 0xff) as f32, ((b >> shift) & 0xff) as f32);
        let v = (t as f64 * (y - x) as f64 + x as f64) as f32 as i32;
        out | (if v as u32 & 0xffff_ff00 != 0 {
            0xff
        } else {
            v as u32
        }) << shift
    })
}

/// Vertex colours the sea draws on a frame (CartD +0x9ac..+0xb84): rows of
/// the source grid scroll by one every `period` frames, blending each row
/// with the next by the fraction of the period elapsed. Positions are
/// copied unchanged.
pub fn sea_colors(cart: &[u8], frame: u32) -> Result<Vec<u32>> {
    let (rel, v) = background_module(cart)?;
    let sea = sea_source(&rel, &v)?;
    let data = rel.data(5)?;
    let n = cart_sqrt(sea.count as f32) as i32;
    let p = sea.period as i32;
    if n <= 0 || n * n > sea.count as i32 || p <= 0 {
        return Err(Error::invalid("unexpected CartD sea grid"));
    }
    let r = frame % (n * p) as u32;
    let shift = r as i32 / p;
    let t = (r as i32 - shift * p) as f32 / p as f32;
    let color = |i: i32| be_word(data, sea.vertex_list + 8 + 16 * i as u32 + 12);
    let mut out = Vec::with_capacity((n * n) as usize);
    for row in 0..n {
        let wrap = |i: i32| if i < n { i } else { i - n };
        let (a, b) = (wrap(row + shift) * n, wrap(row + shift + 1) * n);
        for col in 0..n {
            out.push(blend_argb(color(a + col)?, color(b + col)?, t));
        }
    }
    Ok(out)
}

pub fn course_background(
    exe: &[u8],
    dll: &[u8],
    cart: &[u8],
    name: &str,
) -> Result<CourseBackground> {
    let (rel, v) = background_module(cart)?;
    let sea = sea_source(&rel, &v)?;
    let placements = native_course_placements(exe, dll, name)?;
    let s4 = rel.data(4)?;
    let s5 = rel.data(5)?;
    let mut minimum_y = f32::from_bits(be_word(s4, v[7])?);
    for p in placements.iter().skip(1) {
        if p.position[1] < minimum_y {
            minimum_y = p.position[1];
        }
    }
    Ok(CourseBackground {
        course: courses(exe)?
            .into_iter()
            .find(|c| c.name == name)
            .map(|c| c.name)
            .ok_or_else(|| Error::invalid("unknown kart course"))?,
        minimum_y,
        distant_object: v[1],
        distant_height_offset: f32::from_bits(be_word(s5, v[0])?),
        distant_textures: rel_names(&rel, v[2])?,
        sea_object: v[3],
        sea_scale: f32::from_bits(be_word(s4, v[5])?),
        sea_grid: cart_sqrt(sea.count as f32) as u32,
        sea_period: sea.period,
        sea_textures: rel_names(&rel, v[6])?,
    })
}

#[cfg(test)]
mod background_blend {
    use super::{blend_argb, cart_sqrt};

    #[test]
    fn blend_endpoints_and_truncation() {
        assert_eq!(blend_argb(0x1020_3040, 0xffee_ddcc, 0.0), 0x1020_3040);
        assert_eq!(blend_argb(0x1020_3040, 0xffee_ddcc, 1.0), 0xffee_ddcc);
        // 0 + 0.5 * 255 = 127.5 truncates to 127 in every channel.
        assert_eq!(blend_argb(0, 0xffff_ffff, 0.5), 0x7f7f_7f7f);
        assert_eq!(cart_sqrt(121.0), 11.0);
        assert_eq!(cart_sqrt(0.0), 0.0);
    }
}
