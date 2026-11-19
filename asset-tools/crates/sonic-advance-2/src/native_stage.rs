//! Leaf Forest Act 1 native metatiles and terrain probes.
//! Format/behavior evidence: pinned SAT-R/sa2 tilemap.h, core.h and
//! terrain_collision.c sub_801EF94/sub_801ED24/sub_801EE64/sub_801E4E4.
use crate::{Error, VerifiedRom, graphics};
use serde::Serialize;

pub const WIDTH: i32 = 19_776;
pub const HEIGHT: i32 = 3_072;
pub const SPAWN: (i32, i32) = (96, 655);
const MAP_WIDTH: usize = 206;
const MAP_HEIGHT: usize = 32;
const METATILE_TILES: usize = 12;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct TerrainSample {
    pub extent: i32,
    pub rotation: u8,
    pub tile: u16,
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct TerrainDistance {
    pub distance: i32,
    pub rotation: Option<u8>,
}

pub struct NativeStage<'a> {
    maps: [&'a [u8]; 2],
    metatiles: &'a [u8],
    heights: &'a [u8],
    rotations: &'a [u8],
    flags: &'a [u8],
    tiles: &'a [u8],
    palette: Vec<graphics::PaletteColor>,
}

fn word(bytes: &[u8], offset: usize) -> Result<u16, Error> {
    let raw = crate::bounded_range(bytes, offset, 2)?;
    Ok(u16::from_le_bytes([raw[0], raw[1]]))
}

impl<'a> NativeStage<'a> {
    pub fn leaf_forest(rom: &'a VerifiedRom<'_>) -> Result<Self, Error> {
        let read = |offset, length| rom.range("rom", offset, length);
        let stage = Self {
            maps: [read(0x70de30, 13184)?, read(0x7111b0, 13184)?],
            metatiles: read(0x6f33d0, 109152)?,
            heights: read(0x6f12a8, 7336)?,
            rotations: read(0x6f2f50, 920)?,
            flags: read(0x6f32e8, 232)?,
            tiles: read(0x6ea008, 0x72a0)?,
            palette: graphics::parse_palette(read(0x6e9e08, 512)?, 256)
                .map_err(|e| Error(e.to_string()))?,
        };
        for map in stage.maps {
            for pair in map.as_chunks::<2>().0 {
                let index = u16::from_le_bytes(*pair) as usize;
                if index >= stage.metatiles.len() / (12 * 12 * 2) {
                    return Err(Error("Metatile map index outside native bank".into()));
                }
            }
        }
        for pair in stage.metatiles.as_chunks::<2>().0 {
            let index = (u16::from_le_bytes(*pair) & 1023) as usize;
            if index * 32 + 32 > stage.tiles.len()
                || index * 8 + 8 > stage.heights.len()
                || index >= stage.rotations.len()
            {
                return Err(Error("Native tile dependency outside source bank".into()));
            }
        }
        Ok(stage)
    }

    pub fn tile_at(&self, x: i32, y: i32, layer: u8) -> Result<u16, Error> {
        if !(0..WIDTH).contains(&x) || !(0..HEIGHT).contains(&y) || layer > 1 {
            return Err(Error(
                "Terrain coordinate/layer outside native stage".into(),
            ));
        }
        let tx = x as usize / 8;
        let ty = y as usize / 8;
        let mx = tx / METATILE_TILES;
        let my = ty / METATILE_TILES;
        debug_assert!(my < MAP_HEIGHT);
        let index = word(self.maps[layer as usize], (my * MAP_WIDTH + mx) * 2)? as usize;
        word(self.metatiles, (index * 144 + (ty % 12) * 12 + tx % 12) * 2)
    }

    pub fn vertical_sample(&self, x: i32, y: i32, layer: u8) -> Result<TerrainSample, Error> {
        self.sample(x, y, layer, true)
    }

    pub fn horizontal_sample(&self, x: i32, y: i32, layer: u8) -> Result<TerrainSample, Error> {
        self.sample(x, y, layer, false)
    }

    fn sample(&self, x: i32, y: i32, layer: u8, vertical: bool) -> Result<TerrainSample, Error> {
        if layer & !0x81 != 0 {
            return Err(Error("Unsupported native collision layer bits".into()));
        }
        let x = x.clamp(0, WIDTH - 1);
        let y = y.clamp(0, HEIGHT - 1);
        let tile = self.tile_at(x, y, layer & 1)?;
        let index = (tile & 1023) as usize;
        let mut column = if vertical {
            x as usize & 7
        } else {
            y as usize & 7
        };
        let along_flip = if vertical { 0x400 } else { 0x800 };
        let extent_flip = if vertical { 0x800 } else { 0x400 };
        if tile & along_flip != 0 {
            column = 7 - column;
        }
        let packed = self.heights[index * 8 + column];
        let mut extent = if vertical {
            (packed as i8 >> 4) as i32
        } else {
            ((packed << 4) as i8 >> 4) as i32
        };
        if extent == -8 {
            extent = 8;
        }
        if layer & 0x80 != 0 && (word(self.flags, index / 8 * 2)? >> ((index % 8) * 2)) & 1 != 0 {
            extent = 0;
        }
        if tile & extent_flip != 0 && extent != 8 && extent != 0 {
            extent += if extent > 0 { -8 } else { 8 };
        }
        let mut rotation = self.rotations[index];
        if vertical {
            if tile & 0x400 != 0 {
                rotation = rotation.wrapping_neg();
            }
            if tile & 0x800 != 0 && extent != 0 {
                rotation = 0x80u8.wrapping_neg().wrapping_sub(rotation);
            }
        } else {
            if tile & 0x800 != 0 {
                rotation = 0x80u8.wrapping_neg().wrapping_sub(rotation);
            }
            if tile & 0x400 != 0 && extent != 0 {
                rotation = rotation.wrapping_neg();
            }
        }
        Ok(TerrainSample {
            extent,
            rotation,
            tile,
        })
    }

    /// Exact three-tile probe decisions from native sub_801E4E4. Each sample
    /// clamps source coordinates, while pixel remainder uses the original axis.
    pub fn probe(
        &self,
        x: i32,
        y: i32,
        layer: u8,
        step: i32,
        vertical: bool,
    ) -> Result<TerrainDistance, Error> {
        if step != 8 && step != -8 {
            return Err(Error("Native probe step must be +8 or -8".into()));
        }
        let p0 = if vertical { y } else { x };
        let remainder = (p0 as u32 & 7) as i32;
        let at = |i: i32| {
            let q = p0.wrapping_add(step.wrapping_mul(i));
            if vertical {
                self.vertical_sample(x, q, layer)
            } else {
                self.horizontal_sample(q, y, layer)
            }
        };
        let slope = |h: i32, i: i32| {
            if h > 0 {
                h - 1 - remainder + i * 8
            } else {
                h + remainder + i * 8
            }
        };
        let mut rotation = None;
        let first = at(0)?;
        let distance = if first.extent == 0 {
            let mut found = None;
            for i in 1..=2 {
                let sample = at(i)?;
                if sample.extent != 0 {
                    found = Some((i, sample));
                    break;
                }
            }
            match found {
                None => {
                    if step > 0 {
                        24 - remainder
                    } else {
                        remainder + 17
                    }
                }
                Some((i, sample)) => {
                    rotation = Some(sample.rotation);
                    if sample.extent == 8 {
                        if step > 0 {
                            !remainder + i * 8
                        } else {
                            remainder + (i - 1) * 8
                        }
                    } else {
                        slope(sample.extent, i)
                    }
                }
            }
        } else if first.extent == 8 {
            rotation = Some(first.rotation);
            let previous = at(-1)?;
            if previous.extent == 0 {
                if step > 0 { !remainder } else { remainder - 8 }
            } else if previous.extent != 8 {
                rotation = Some(previous.rotation);
                slope(previous.extent, -1)
            } else {
                rotation = Some(previous.rotation);
                let before = at(-2)?;
                if before.extent == 0 {
                    if step > 0 {
                        -9 - remainder
                    } else {
                        remainder - 16
                    }
                } else {
                    rotation = Some(before.rotation);
                    if before.extent == 8 {
                        if step > 0 {
                            !remainder - 16
                        } else {
                            remainder - 24
                        }
                    } else {
                        slope(before.extent, -2)
                    }
                }
            }
        } else {
            rotation = Some(first.rotation);
            slope(first.extent, 0)
        };
        Ok(TerrainDistance { distance, rotation })
    }

    /// Native sub_801E6D4's extended lookahead. Its backward search has the
    /// original repeated index-two branch; do not replace it with a geometric
    /// distance formula. Jump animation uses this probe before landing.
    pub fn long_probe(
        &self,
        x: i32,
        y: i32,
        layer: u8,
        step: i32,
        vertical: bool,
    ) -> Result<TerrainDistance, Error> {
        if step != 8 && step != -8 {
            return Err(Error("Native probe step must be +8 or -8".into()));
        }
        let axis = if vertical { y } else { x };
        let remainder = (axis as u32 & 7) as i32;
        let at = |i: i32| {
            let q = axis.wrapping_add(step.wrapping_mul(i));
            if vertical {
                self.vertical_sample(x, q, layer)
            } else {
                self.horizontal_sample(q, y, layer)
            }
        };
        let slope = |h: i32, i: i32| {
            if h > 0 {
                h - 1 - remainder + i * 8
            } else {
                h + remainder + i * 8
            }
        };
        let full = |i: i32| {
            if step > 0 {
                !remainder + i * 8
            } else {
                remainder + (i - 1) * 8
            }
        };
        let first = at(0)?;
        if first.extent == 0 {
            for i in 1..=5 {
                let sample = at(i)?;
                if sample.extent != 0 {
                    return Ok(TerrainDistance {
                        distance: if sample.extent == 8 {
                            full(i)
                        } else {
                            slope(sample.extent, i)
                        },
                        rotation: Some(sample.rotation),
                    });
                }
            }
            return Ok(TerrainDistance {
                distance: if step > 0 {
                    48 - remainder
                } else {
                    remainder + 41
                },
                rotation: None,
            });
        }
        let mut rotation = first.rotation;
        if first.extent == 8 {
            for back in 1..=6 {
                let sample = at(-back)?;
                let index = if back >= 3 { back - 1 } else { back };
                if sample.extent == 0 {
                    return Ok(TerrainDistance {
                        distance: if step > 0 {
                            -(index - 1) * 8 - 1 - remainder
                        } else {
                            remainder - index * 8
                        },
                        rotation: Some(rotation),
                    });
                }
                rotation = sample.rotation;
                if sample.extent != 8 {
                    return Ok(TerrainDistance {
                        distance: slope(sample.extent, -index),
                        rotation: Some(rotation),
                    });
                }
            }
            return Ok(TerrainDistance {
                distance: full(-5),
                rotation: Some(rotation),
            });
        }
        Ok(TerrainDistance {
            distance: slope(first.extent, 0),
            rotation: Some(rotation),
        })
    }

    pub fn render_view(
        &self,
        camera_x: i32,
        camera_y: i32,
        width: usize,
        height: usize,
    ) -> Result<Vec<u8>, Error> {
        let pixels = width
            .checked_mul(height)
            .filter(|&n| n <= 640 * 480)
            .ok_or_else(|| Error("Stage viewport limit".into()))?;
        if width == 0 || height == 0 {
            return Err(Error("Empty stage viewport".into()));
        }
        let clear = self.palette[0].rgb8();
        let mut output = Vec::with_capacity(pixels * 4);
        for _ in 0..pixels {
            output.extend_from_slice(&[clear[0], clear[1], clear[2], 255]);
        }
        for layer in [1, 0] {
            for sy in 0..height {
                let y = camera_y.saturating_add(sy as i32);
                if !(0..HEIGHT).contains(&y) {
                    continue;
                }
                for sx in 0..width {
                    let x = camera_x.saturating_add(sx as i32);
                    if !(0..WIDTH).contains(&x) {
                        continue;
                    }
                    let tile = self.tile_at(x, y, layer)?;
                    let px = if tile & 0x400 != 0 {
                        7 - (x as usize & 7)
                    } else {
                        x as usize & 7
                    };
                    let py = if tile & 0x800 != 0 {
                        7 - (y as usize & 7)
                    } else {
                        y as usize & 7
                    };
                    let byte = self.tiles[(tile as usize & 1023) * 32 + py * 4 + px / 2];
                    let nibble = if px & 1 == 0 { byte & 15 } else { byte >> 4 };
                    if nibble == 0 {
                        continue;
                    }
                    let color = self.palette[(tile as usize >> 12) * 16 + nibble as usize].rgb8();
                    let at = (sy * width + sx) * 4;
                    output[at..at + 3].copy_from_slice(&color);
                }
            }
        }
        Ok(output)
    }
}
