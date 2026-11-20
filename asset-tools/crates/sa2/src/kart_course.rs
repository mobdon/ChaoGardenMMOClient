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

/// Static terrain for an ordered course: every occurrence's native display
/// and Basic collision objects from the pinned Steam Data_DLL, placed by
/// course_layout from the origin. Visual entries precede collision entries.
pub fn course_terrain(dll: &[u8], ids: &[u8]) -> Result<crate::stage::Stage> {
    if sha256(dll) != crate::board_motion::PC_DLL_SHA256 {
        return Err(Error::invalid("unsupported kart DLL revision"));
    }
    let pe = read_pe(dll)?;
    let table = read_course_table(dll)?;
    let poses = course_layout(&table, ids, CoursePose::default())?;
    let occurrences: Vec<_> = ids
        .iter()
        .zip(poses)
        .map(|(&id, pose)| {
            let piece = &table.pieces[usize::from(id)];
            crate::city_land::KartOccurrence {
                display: piece.display,
                collision: piece.collision,
                pose,
            }
        })
        .collect();
    crate::city_land::win32_kart_course(dll, &pe, &occurrences)
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

#[cfg(test)]
mod terrain_reference {
    use super::*;

    /// The assembled land view must bake to exactly the triangles of the
    /// direct per-piece walk: same piece order, strip order, source indices
    /// and winding, with positions equal up to the binary32 pose rounding.
    #[test]
    #[ignore = "requires installed Steam SA2"]
    fn story_terrain_matches_piece_walk() {
        let root = std::path::PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap());
        let exe = std::fs::read(root.join("sonic2app.exe")).unwrap();
        let dll = std::fs::read(root.join("resource/gd_PC/DLL/Win32/Data_DLL.dll")).unwrap();
        let pe = read_pe(&dll).unwrap();
        let table = read_course_table(&dll).unwrap();
        for course in courses(&exe).unwrap() {
            let ids = course.piece_ids;
            let stage = course_terrain(&dll, ids).unwrap();
            let shown = ids
                .iter()
                .filter(|&&i| table.pieces[usize::from(i)].display.is_some())
                .count();
            let solid: Vec<_> = ids
                .iter()
                .zip(course_layout(&table, ids, CoursePose::default()).unwrap())
                .filter(|(&i, _)| table.pieces[usize::from(i)].collision.is_some())
                .collect();
            assert_eq!(stage.visual_count as usize, shown, "{}", course.name);
            assert_eq!(stage.entries.len(), shown + solid.len());
            let world = crate::collision::bake_stage(&stage, 1.).unwrap();
            assert_eq!(world.meshes.len(), solid.len());
            let mut worst = 0f64;
            let mut triangles = 0;
            for (mesh, (&id, pose)) in world.meshes.iter().zip(&solid) {
                let mut expected = vec![];
                visit_piece_triangles(&dll, &pe, &table.pieces[usize::from(id)], *pose, |t| {
                    expected.push(t)
                })
                .unwrap();
                assert_eq!(mesh.triangles.len(), expected.len());
                for (tri, t) in mesh.triangles.iter().zip(&expected) {
                    assert_eq!(tri.map(|i| i as u16), t.vertex_indices);
                    for k in 0..3 {
                        let p = mesh.vertices[tri[k] as usize];
                        for c in 0..3 {
                            worst = worst.max((p[c] - t.points[k][c]).abs());
                        }
                    }
                }
                triangles += expected.len();
            }
            eprintln!(
                "{}: entries={} visual={} collision={} triangles={triangles} max_position_delta={worst}",
                course.name,
                stage.entries.len(),
                shown,
                solid.len()
            );
            assert!(worst < 0.01, "{}: {worst}", course.name);
            match course.name {
                "101" => assert_eq!(triangles, 13643),
                "280" => assert_eq!(triangles, 18439),
                _ => {}
            }
        }
    }
    /// Independent check of the display half against SA Tools 786 GC models
    /// split from the same DLL: native root transform words, hierarchy size,
    /// opaque/translucent mesh counts (alpha passes) and attachment sphere.
    #[test]
    #[ignore = "requires Steam DLL and independent SA Tools kart models"]
    fn story_display_entries_match_sa_tools() {
        let root = std::path::PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap());
        let models =
            std::path::PathBuf::from(std::env::var_os("SA2_KART_MODEL_REFERENCE").unwrap());
        let exe = std::fs::read(root.join("sonic2app.exe")).unwrap();
        let dll = std::fs::read(root.join("resource/gd_PC/DLL/Win32/Data_DLL.dll")).unwrap();
        let table = read_course_table(&dll).unwrap();
        let d = crate::model::Reader { data: &dll };
        for course in courses(&exe).unwrap().into_iter().take(2) {
            let stage = course_terrain(&dll, course.piece_ids).unwrap();
            let poses = course_layout(&table, course.piece_ids, CoursePose::default()).unwrap();
            let shown: Vec<_> = course
                .piece_ids
                .iter()
                .zip(poses)
                .filter(|(&i, _)| table.pieces[usize::from(i)].display.is_some())
                .collect();
            let (mut translucent, mut opaque) = (0, 0);
            for (entry, (&id, pose)) in stage.entries.iter().zip(&shown) {
                let bytes = std::fs::read(models.join(format!("{id:03}.sa2bmdl"))).unwrap();
                assert_eq!(&bytes[..8], b"SA2BMDL");
                let r = crate::model::Reader { data: &bytes };
                let count = |reader: &crate::model::Reader<'_>, start: usize, bias: usize| {
                    let (mut n, mut stack) = (0, vec![start]);
                    while let Some(o) = stack.pop() {
                        n += 1;
                        for j in [44, 48] {
                            let p = reader.u32(o + j).unwrap() as usize;
                            if p != 0 {
                                stack.push(p - bias);
                            }
                        }
                    }
                    n
                };
                let m = r.u32(8).unwrap() as usize;
                let wrapper = &entry.nodes[0];
                let native = &entry.nodes[1];
                assert_eq!(wrapper.attach_offset, 0);
                assert_eq!(wrapper.rotation_bams, [0, i32::from(pose.heading), 0]);
                assert_eq!(entry.nodes.len(), 1 + count(&r, m, 0));
                assert_eq!(native.flags, r.u32(m).unwrap());
                for k in 0..3 {
                    assert_eq!(native.position[k].to_bits(), r.u32(m + 8 + 4 * k).unwrap());
                    assert_eq!(
                        native.rotation_bams[k] as u32,
                        r.u32(m + 20 + 4 * k).unwrap()
                    );
                    assert_eq!(native.scale[k].to_bits(), r.u32(m + 32 + 4 * k).unwrap());
                }
                let a = r.u32(m + 4).unwrap() as usize;
                let n = native.attach_offset as usize - 16;
                for k in [16, 18] {
                    assert_eq!(d.u16(n + k).unwrap(), r.u16(a + k).unwrap());
                }
                for k in 0..4 {
                    assert_eq!(
                        d.u32(n + 20 + 4 * k).unwrap(),
                        r.u32(a + 20 + 4 * k).unwrap()
                    );
                }
                opaque += usize::from(r.u16(a + 16).unwrap());
                translucent += usize::from(r.u16(a + 18).unwrap());
                assert_eq!(entry.bounds_radius.to_bits(), r.u32(a + 32).unwrap());
            }
            eprintln!(
                "{}: display entries={} opaque meshes={opaque} translucent meshes={translucent}",
                course.name,
                shown.len()
            );
        }
    }
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
                translucent_texels: image.rgba.as_chunks::<4>().0.iter().filter(|p| p[3] < 255).count(),
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

#[cfg(test)]
mod materials_reference {
    use super::*;

    /// Mesh texture and blend parameters decoded from the Steam DLL equal the
    /// ones in SA Tools 786 GC models split from the same DLL, piece by piece.
    #[test]
    #[ignore = "requires Steam SA2 and independent SA Tools kart models"]
    fn story_materials_match_sa_tools() {
        let root = std::path::PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap());
        let models =
            std::path::PathBuf::from(std::env::var_os("SA2_KART_MODEL_REFERENCE").unwrap());
        let gd = root.join("resource/gd_PC");
        let exe = std::fs::read(root.join("sonic2app.exe")).unwrap();
        let dll = std::fs::read(gd.join("DLL/Win32/Data_DLL.dll")).unwrap();
        let cart = std::fs::read(gd.join("CartD.rel")).unwrap();
        let pak = std::fs::read(gd.join("PRS/course.pak")).unwrap();
        for (name, meshes, textures) in [("101", 3981, 42), ("280", 4853, 45)] {
            let m = course_materials(&exe, &dll, &cart, &pak, name).unwrap();
            assert_eq!(m.texture_names.len(), 74);
            assert_eq!((m.meshes, m.textures.len()), (meshes, textures));
            for piece in &m.pieces {
                let bytes =
                    std::fs::read(models.join(format!("{:03}.sa2bmdl", piece.piece_id))).unwrap();
                let r = crate::model::Reader { data: &bytes };
                let a = r.u32(r.u32(8).unwrap() as usize + 4).unwrap() as usize;
                let mut expected = vec![];
                for (list, count) in [(8, 16), (12, 18)] {
                    let base = r.u32(a + list).unwrap() as usize;
                    for k in 0..r.u16(a + count).unwrap() as usize {
                        let p = r.u32(base + 16 * k).unwrap() as usize;
                        let mut texture = None;
                        let mut blend = None;
                        for j in 0..r.u32(base + 16 * k + 4).unwrap() as usize {
                            match r.u32(p + 8 * j).unwrap() {
                                4 => blend = Some(r.u32(p + 8 * j + 4).unwrap()),
                                8 => texture = Some(r.u32(p + 8 * j + 4).unwrap()),
                                _ => {}
                            }
                        }
                        expected.push((count == 18, texture, blend));
                    }
                }
                let ours: Vec<_> = piece
                    .meshes
                    .iter()
                    .map(|mesh| {
                        let word = mesh
                            .texture_slot
                            .map(|s| u32::from(s) | u32::from(mesh.texture_flags.unwrap()) << 16);
                        (mesh.translucent_list, word, mesh.blend_alpha)
                    })
                    .collect();
                assert_eq!(ours, expected, "route {name} piece {}", piece.piece_id);
            }
        }
    }
}
