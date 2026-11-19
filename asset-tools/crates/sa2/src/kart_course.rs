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
