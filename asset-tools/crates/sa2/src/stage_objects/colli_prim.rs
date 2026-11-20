//! The invisible collision primitives SPHERE, CCYL, CCUBE, CWALL and
//! CCIRCLE (object list rows 17 to 21).
//!
//! stg42D.rel section 1: the shared size helper at 0xBE94 and the five
//! exec callbacks 0xBEF8 (SPHERE), 0xBF9C (CCYL), 0xC040 (CCUBE), 0xC0E4
//! (CWALL) and 0xC188 (CCIRCLE). Each exec is the same three-mode machine:
//!
//! * the range check comes first: `CheckRangeOut(task)` (0x800655F4), or for
//!   CCIRCLE `CheckRangeOutWithR(task, 9000000.0)` (0x80065498, radius from
//!   section 4 +0x4AC); when it reports the task out of range nothing else
//!   runs this frame;
//! * mode 0 (twp +0, read as a signed byte): `CCL_Init(task, row, 1, id)`
//!   (0x8000F8B8) with the object's CollisionData row (section 5) and list id
//!   4 (SPHERE, CCYL, CCUBE) or 8 (CWALL, CCIRCLE); then 0xBE94 sets the
//!   collision info's a, b, c (`cwp->info +0x14..+0x1C`) to `10.0 + Scale`
//!   (10.0 from section 4 +0x4A8, Scale at twp +0x20..+0x28, single-precision
//!   adds) and calls fn_800068E4(cwp), which runs `CCL_CalcRange` on it;
//!   mode becomes 1;
//! * mode 1: `CCL_Entry(task)` (0x8000F4EC);
//! * any other mode (negative, or above 1) is reset to 0 and nothing else
//!   happens that frame.
//!
//! The CollisionData rows are the stage module's own data (stg42D section 5
//! +0xDB80, +0xDBB0, +0xDBE0, +0xDC10, +0xDC40); every stage carries its own
//! copy at its own offset, so a stage passes the row it found in its REL.
use super::host::{Event, ObjectHost};

/// The routines this file ports (stg42D.rel section 1).
pub const ROUTINES: &[super::Routine] = &[
    super::Routine {
        name: "colli_prim sizes (0xBE94)",
        stg42d: 0xbe94,
        len: 0x64,
        masked_sha256: "8657ce1582b2b36e517bdcaf52d38f88be786993f0c94ca001e4fc384f8de5bd",
    },
    super::Routine {
        name: "SPHERE exec",
        stg42d: 0xbef8,
        len: 0xa4,
        masked_sha256: "66985a04e61f8865f532618c1b8f0c8d6743f49b12b3b3f2783bd223b0b6c31a",
    },
    super::Routine {
        name: "CCYL exec",
        stg42d: 0xbf9c,
        len: 0xa4,
        masked_sha256: "9b231f207df1a4a67c967c565d246cc8c70deec2a2443026a21ee4a0bd587cbb",
    },
    super::Routine {
        name: "CCUBE exec",
        stg42d: 0xc040,
        len: 0xa4,
        masked_sha256: "b4531001eb722c66f5ef4607115fbf16285cebabb72a57428af866f474940750",
    },
    super::Routine {
        name: "CWALL exec",
        stg42d: 0xc0e4,
        len: 0xa4,
        masked_sha256: "9dfd705ba49f046e3c56b70aac800d98522c6b59ac84a971485c27bc4e37c6c1",
    },
    super::Routine {
        name: "CCIRCLE exec",
        stg42d: 0xc188,
        len: 0xb0,
        masked_sha256: "f68cf39c8d72b584310285e9840e2607f62bca9883738ffb1900f49edaf72204",
    },
];

/// Which primitive (and so which list id and range check).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Primitive {
    Sphere,
    Cylinder,
    Cube,
    Wall,
    Circle,
}

/// The size every primitive adds to its Scale (stg42D section 4 +0x4A8).
pub const BASE_SIZE: f32 = 10.0;
/// CCIRCLE's CheckRangeOutWithR radius (stg42D section 4 +0x4AC).
pub const CIRCLE_RANGE: f32 = 9_000_000.0;

impl Primitive {
    /// The object list name (rows 17 to 21).
    pub fn name(self) -> &'static str {
        match self {
            Self::Sphere => "SPHERE",
            Self::Cylinder => "CCYL",
            Self::Cube => "CCUBE",
            Self::Wall => "CWALL",
            Self::Circle => "CCIRCLE",
        }
    }

    /// The exec callback's offset in stg42D.rel section 1.
    pub fn stg42d_exec(self) -> u32 {
        match self {
            Self::Sphere => 0xbef8,
            Self::Cylinder => 0xbf9c,
            Self::Cube => 0xc040,
            Self::Wall => 0xc0e4,
            Self::Circle => 0xc188,
        }
    }

    /// The collision list id CCL_Init is given.
    pub fn list_id(self) -> u8 {
        match self {
            Self::Sphere | Self::Cylinder | Self::Cube => 4,
            Self::Wall | Self::Circle => 8,
        }
    }

    /// Offset, inside the exec callback, of the `lis`/`addi` pair that
    /// loads the CollisionData row (the stage REL relocates both halves).
    pub fn row_reloc(self) -> u32 {
        match self {
            Self::Circle => 0x5c,
            _ => 0x50,
        }
    }
}

/// The primitive's task state the exec reads and writes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrimitiveTask {
    /// twp +0 (signed byte).
    pub mode: i8,
    /// twp +0x20..+0x28 (Scale from the SET entry).
    pub scale: [f32; 3],
}

/// The a, b, c sizes 0xBE94 writes into the collision info.
pub fn sizes(scale: [f32; 3]) -> [f32; 3] {
    scale.map(|s| BASE_SIZE + s)
}

/// One exec frame. `row` is the CollisionData row's section-5 offset in the
/// stage REL the object came from.
pub fn exec(kind: Primitive, task: &mut PrimitiveTask, row: u32, host: &mut impl ObjectHost) {
    let out = match kind {
        Primitive::Circle => host.range_out_with_r(CIRCLE_RANGE),
        _ => host.range_out(),
    };
    if out {
        return;
    }
    match task.mode {
        0 => {
            host.event(Event::CollisionInit {
                rows: row,
                count: 1,
                list: kind.list_id(),
            });
            host.event(Event::CollisionSizes(sizes(task.scale)));
            host.event(Event::CollisionCalcRange);
            task.mode = 1;
        }
        1 => host.event(Event::CollisionEntry),
        _ => task.mode = 0,
    }
}
