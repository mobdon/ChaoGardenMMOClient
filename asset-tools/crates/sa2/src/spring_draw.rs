//! Matrix and native model submissions from the four GSNE8P spring display callbacks.
//! Node numbers refer to SpringGjObjN (GC) or SpringObjN (chunk).
use crate::spring::{SpringKind, StageSpring};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Gc,
    Chunk,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Draw {
    BindSpringTextures,
    Push,
    Pop(u8),
    Translate([f32; 3]),
    RotateZ(i32),
    RotateX(i32),
    RotateY(i32),
    Scale([f32; 3]),
    TranslateNode(u8),
    GcBaseModel(u8),
    GcNodeModel(u8),
    /// ds_DrawModelClip(SpringObjArr[row][3]).
    ChunkModel(u8),
}

/// Invoke when the renderer schedules the display callback. Resolve node positions,
/// models and textures from the matching native asset set.
pub fn display(s: &StageSpring, format: Format, mut emit: impl FnMut(Draw)) {
    use Draw::*;
    for op in [
        BindSpringTextures,
        Push,
        Translate(s.position),
        RotateZ(s.angles[2]),
        RotateX(s.angles[0]),
        RotateY(s.angles[1]),
    ] {
        emit(op);
    }
    if s.wait_timer != 0 {
        emit(Scale([1.0 / f32::from(s.wait_timer); 3]));
    }
    let coil = Scale([0.98, 1.0 + s.compression, 0.98]);
    let lift = Translate([0.0, 4.0 * s.compression, 0.0]);
    let (base, coil_node, coil_model, head_node, head_model) = match (s.kind, format) {
        (SpringKind::A, Format::Gc) => (GcBaseModel(1), 3, GcNodeModel(3), 2, GcNodeModel(2)),
        (SpringKind::A, Format::Chunk) => (ChunkModel(0), 2, ChunkModel(1), 1, ChunkModel(2)),
        (SpringKind::B, Format::Gc) => (GcBaseModel(2), 4, GcNodeModel(4), 1, GcNodeModel(1)),
        (SpringKind::B, Format::Chunk) => (ChunkModel(3), 3, ChunkModel(4), 4, ChunkModel(5)),
    };
    emit(base);
    match s.kind {
        SpringKind::A => {
            for op in [
                TranslateNode(coil_node),
                coil,
                coil_model,
                TranslateNode(head_node),
                lift,
                head_model,
                Pop(1),
            ] {
                emit(op);
            }
        }
        SpringKind::B => {
            for op in [
                Push,
                TranslateNode(coil_node),
                Push,
                TranslateNode(head_node),
                lift,
                head_model,
                Pop(1),
                coil,
                coil_model,
                Pop(2),
            ] {
                emit(op);
            }
        }
    }
}
