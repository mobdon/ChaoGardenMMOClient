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
