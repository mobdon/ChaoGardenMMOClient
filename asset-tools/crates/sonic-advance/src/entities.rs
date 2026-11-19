//! ASOE spatial placement tables and single-player collision-plane triggers.
use crate::{binary, Error, Result};

const MAX_LAYOUT_BYTES: usize = 0x40000;
const STAGES: usize = 18;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityClass {
    Interactable,
    ItemBox,
    Enemy,
}

impl EntityClass {
    pub fn table_offset(self) -> usize {
        match self {
            Self::Interactable => 0xb7f94,
            Self::ItemBox => 0xb7fdc,
            Self::Enemy => 0xb8024,
        }
    }

    fn stride(self) -> usize {
        if self == Self::ItemBox {
            3
        } else {
            7
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    pub record_offset: usize,
    pub region_x: usize,
    pub region_y: usize,
    pub x: i32,
    pub y: i32,
    pub kind: u8,
    /// Raw union bytes. Item boxes have no payload and expose four zeroes.
    pub data: [u8; 4],
}

#[derive(Clone, Debug)]
pub struct EntityLayout {
    pub width_regions: usize,
    pub height_regions: usize,
    pub records: Vec<Placement>,
}

impl EntityLayout {
    pub fn parse(bytes: &[u8], class: EntityClass) -> Result<Self> {
        if bytes.len() > MAX_LAYOUT_BYTES || binary::u32_at(bytes, 0)? as usize >> 8 != bytes.len()
        {
            return Err(Error("invalid entity layout size".into()));
        }
        let width = binary::u32_at(bytes, 4)? as usize;
        let height = binary::u32_at(bytes, 8)? as usize;
        if !(1..=256).contains(&width) || !(1..=256).contains(&height) {
            return Err(Error("invalid entity region dimensions".into()));
        }
        let regions = width * height;
        binary::range(bytes, 12, regions * 4)?;
        let mut records = Vec::new();
        let mut previous_end = 12 + regions * 4;
        for region in 0..regions {
            let relative = binary::u32_at(bytes, 12 + region * 4)? as usize;
            if relative == 0 {
                continue;
            }
            let mut pos = relative
                .checked_add(4)
                .ok_or_else(|| Error("entity list offset overflow".into()))?;
            if pos < previous_end {
                return Err(Error("entity list overlaps preceding data".into()));
            }
            loop {
                let x = binary::range(bytes, pos, 1)?[0];
                if x == 255 {
                    break;
                }
                if x >= 253 {
                    return Err(Error("unexpected mutable entity marker".into()));
                }
                let record = binary::range(bytes, pos, class.stride())?;
                let mut data = [0; 4];
                if class != EntityClass::ItemBox {
                    data.copy_from_slice(&record[3..]);
                }
                records.push(Placement {
                    record_offset: pos,
                    region_x: region % width,
                    region_y: region / width,
                    x: (region % width * 256 + usize::from(x) * 8) as i32,
                    y: (region / width * 256 + usize::from(record[1]) * 8) as i32,
                    kind: record[2],
                    data,
                });
                pos += record.len();
            }
            previous_end = pos + 1;
        }
        Ok(Self {
            width_regions: width,
            height_regions: height,
            records,
        })
    }

    fn load_verified(rom: &[u8], stage: usize, class: EntityClass) -> Result<Self> {
        let pointer = binary::u32_at(rom, class.table_offset() + stage * 4)?;
        let offset = binary::rom_offset(pointer, rom.len(), 4)?;
        let source = &rom[offset..];
        if source.first() != Some(&0x30) {
            return Err(Error("expected native RLE entity layout".into()));
        }
        Self::parse(&binary::decompress(source, MAX_LAYOUT_BYTES)?.bytes, class)
    }
}

#[derive(Clone, Debug)]
pub struct StageEntities {
    pub interactables: EntityLayout,
    pub item_boxes: EntityLayout,
    pub enemies: EntityLayout,
}

impl StageEntities {
    pub fn load(rom: &[u8], stage: usize) -> Result<Self> {
        if stage >= STAGES {
            return Err(Error("invalid ASOE entity stage".into()));
        }
        binary::verify_asoe(rom)?;
        Ok(Self {
            interactables: EntityLayout::load_verified(rom, stage, EntityClass::Interactable)?,
            item_boxes: EntityLayout::load_verified(rom, stage, EntityClass::ItemBox)?,
            enemies: EntityLayout::load_verified(rom, stage, EntityClass::Enemy)?,
        })
    }

    /// Apply the normal stage trigger at the integer player center. Signed data
    /// bytes 0 and 1 are unused; bytes 2 and 3 give unsigned extents in tiles.
    pub fn collision_layer_at(&self, x: i32, y: i32, mut current: u8) -> u8 {
        for p in &self.interactables.records {
            current = p.apply_layer(x, y, current);
        }
        current
    }
}

impl Placement {
    fn apply_layer(&self, x: i32, y: i32, current: u8) -> u8 {
        if matches!(self.kind, 17 | 18)
            && (self.x..=self.x.saturating_add(i32::from(self.data[2]) * 8)).contains(&x)
            && (self.y..=self.y.saturating_add(i32::from(self.data[3]) * 8)).contains(&y)
        {
            (current & !1) | u8::from(self.kind == 18)
        } else {
            current
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct LayerTriggers {
    previous_camera: Option<(i32, i32)>,
    active: Vec<usize>,
}

#[derive(Clone, Copy)]
struct CameraRect {
    left: i32,
    right: i32,
    top: i32,
    bottom: i32,
}

impl CameraRect {
    fn full((x, y): (i32, i32)) -> Self {
        Self {
            left: x.saturating_sub(128),
            right: x.saturating_add(368),
            top: y.saturating_sub(128),
            bottom: y.saturating_add(288),
        }
    }
    fn contains(&self, p: &Placement) -> bool {
        (self.left..=self.right).contains(&p.x) && (self.top..=self.bottom).contains(&p.y)
    }
}

impl LayerTriggers {
    fn spawn(&mut self, layout: &EntityLayout, mut rect: CameraRect) {
        let max_x = (layout.width_regions * 256) as i32 - 1;
        let max_y = (layout.height_regions * 256) as i32 - 1;
        rect.left = rect.left.clamp(0, max_x);
        rect.right = rect.right.clamp(0, max_x);
        rect.top = rect.top.clamp(0, max_y);
        rect.bottom = rect.bottom.clamp(0, max_y);
        if rect.left == rect.right || rect.top == rect.bottom {
            return;
        }
        for (index, p) in layout.records.iter().enumerate() {
            // The manager iterates the record's *source region*, which can
            // differ from the region containing its quantized world point.
            if matches!(p.kind, 17 | 18)
                && p.region_x >= ((rect.left >> 8) as usize)
                && (p.region_x * 256) < rect.right as usize
                && p.region_y >= ((rect.top >> 8) as usize)
                && (p.region_y * 256) < rect.bottom as usize
                && rect.contains(p)
                && !self.active.contains(&index)
            {
                self.active.push(index);
            }
        }
    }

    /// One manager scan, then active trigger tasks in equal-priority insertion order.
    /// Keep one instance per stage, and pass the current camera before its next update.
    pub fn update(
        &mut self,
        layout: &EntityLayout,
        camera: (i32, i32),
        player: (i32, i32),
        mut current: u8,
    ) -> u8 {
        let full = CameraRect::full(camera);
        match self.previous_camera {
            Some(prev) if prev.0.abs_diff(camera.0) <= 248 && prev.1.abs_diff(camera.1) <= 208 => {
                let old = CameraRect::full(prev);
                if prev.0 != camera.0 {
                    let (left, right) = if camera.0 > prev.0 {
                        (old.right, full.right)
                    } else {
                        (full.left, old.left)
                    };
                    self.spawn(
                        layout,
                        CameraRect {
                            left,
                            right,
                            ..full
                        },
                    );
                }
                if prev.1 != camera.1 {
                    let (top, bottom) = if camera.1 > prev.1 {
                        (old.bottom, full.bottom)
                    } else {
                        (full.top, old.top)
                    };
                    self.spawn(
                        layout,
                        CameraRect {
                            top,
                            bottom,
                            ..full
                        },
                    );
                }
            }
            _ => self.spawn(layout, full),
        }
        self.previous_camera = Some(camera);
        self.active.retain(|&index| {
            let p = &layout.records[index];
            // The native task tests the player before its camera destructor.
            current = p.apply_layer(player.0, player.1, current);
            full.contains(p)
        });
        current
    }
}
