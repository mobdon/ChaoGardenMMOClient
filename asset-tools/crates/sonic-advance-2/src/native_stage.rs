//! A2NE native stage foregrounds and terrain probes.
//! Format/behavior evidence: pinned SAT-R/sa2 tilemap.h, core.h and
//! terrain_collision.c sub_801EF94/sub_801ED24/sub_801EE64/sub_801E4E4.
use crate::{Error, VerifiedRom, graphics};
use serde::Serialize;

pub const WIDTH: i32 = 19_776;
pub const HEIGHT: i32 = 3_072;
pub const SPAWN: (i32, i32) = (96, 655);
const TILEMAP_TABLE: usize = 0xd5ce4;
const COLLISION_TABLE: usize = 0xd62d8;
const SPAWN_TABLE: usize = 0xd6374;

// The first 31 entries have paired foreground headers. Slot 3 in each
// course zone selects the unused Leaf Forest foreground.
pub const TERRAIN_LEVELS: u8 = 31;

pub fn is_course_act(level: u8) -> bool {
    level < 28 && level % 4 < 2
}
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
    pub level: u8,
    pub spawn: (i32, i32),
    width: i32,
    height: i32,
    map_width: usize,
    maps: [&'a [u8]; 2],
    animation: &'a [u8],
    frame_bytes: usize,
    frame_count: u8,
    frame_delay: u8,
    metatiles: &'a [u8],
    heights: &'a [u8],
    rotations: &'a [u8],
    flags: &'a [u8],
    tiles: &'a [u8],
    palette: Vec<graphics::PaletteColor>,
    leaf_backgrounds: Option<[Box<[u8]>; 2]>,
}

fn word(bytes: &[u8], offset: usize) -> Result<u16, Error> {
    let raw = crate::bounded_range(bytes, offset, 2)?;
    Ok(u16::from_le_bytes([raw[0], raw[1]]))
}

fn offset(pointer: u32) -> Result<usize, Error> {
    if !(0x08000000..0x09000000).contains(&pointer) {
        return Err(Error("Stage pointer outside A2NE cartridge".into()));
    }
    Ok((pointer - 0x08000000) as usize)
}

fn long(bytes: &[u8], position: usize) -> Result<u32, Error> {
    let raw = crate::bounded_range(bytes, position, 4)?;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

impl<'a> NativeStage<'a> {
    pub fn leaf_forest(rom: &'a VerifiedRom<'_>) -> Result<Self, Error> {
        Self::for_level(rom, 0)
    }

    pub fn dimensions(&self) -> (i32, i32) {
        (self.width, self.height)
    }

    pub fn for_level(rom: &'a VerifiedRom<'_>, level: u8) -> Result<Self, Error> {
        if level >= TERRAIN_LEVELS {
            return Err(Error("Level has no native A2NE foreground pair".into()));
        }
        let id = usize::from(level);
        let bytes = rom.rom_bytes();
        let header = |table_index, length| {
            crate::bounded_range(bytes, offset(long(bytes, table_index)?)?, length)
        };
        let front = header(TILEMAP_TABLE + 12 * id, 36)?;
        let back = header(TILEMAP_TABLE + 12 * id + 4, 36)?;
        let collision = header(COLLISION_TABLE + 4 * id, 36)?;
        let (mw, mh) = (word(front, 32)?, word(front, 34)?);
        let width = i32::from(mw) * 96;
        let height = i32::from(mh) * 96;
        if mw == 0
            || mh == 0
            || front[..28] != back[..28]
            || front[32..] != back[32..]
            || word(front, 0)? != 12
            || word(front, 2)? != 12
            || word(front, 20)? != 0
            || word(front, 22)? != 256
            || collision[24..28] != front[32..36]
            || long(collision, 28)? != width as u32
            || long(collision, 32)? != height as u32
            || long(collision, 8)? != long(front, 24)?
            || long(collision, 12)? != long(front, 28)?
            || long(collision, 16)? != long(back, 28)?
        {
            return Err(Error(
                "Inconsistent A2NE foreground/collision descriptors".into(),
            ));
        }
        let map_bytes = usize::from(mw) * usize::from(mh) * 2;
        let bank =
            |desc: &[u8], field, len| crate::bounded_range(bytes, offset(long(desc, field)?)?, len);
        let maps = [bank(front, 28, map_bytes)?, bank(back, 28, map_bytes)?];
        let last_meta = maps
            .iter()
            .flat_map(|map| map.as_chunks::<2>().0)
            .map(|v| u16::from_le_bytes(*v))
            .max()
            .unwrap_or(0);
        let metatiles = bank(front, 24, (usize::from(last_meta) + 1) * 144 * 2)?;
        let last_tile = metatiles
            .as_chunks::<2>()
            .0
            .iter()
            .map(|v| u16::from_le_bytes(*v) & 1023)
            .max()
            .unwrap_or(0);
        let tile_count = usize::from(last_tile) + 1;
        let tile_bytes = long(front, 12)? as usize;
        let tiles = bank(front, 8, tile_bytes)?;
        if tile_bytes < tile_count * 32 {
            return Err(Error(
                "Stage metatile refers outside native graphics".into(),
            ));
        }
        let frame_bytes = usize::from(word(front, 4)?);
        let frame_count = front[6];
        let frame_delay = front[7];
        if frame_count > 0 && (frame_bytes == 0 || frame_bytes > tile_bytes || frame_delay == 0) {
            return Err(Error("Invalid native animated tileset".into()));
        }
        let animation = crate::bounded_range(
            bytes,
            offset(long(front, 8)?)? + tile_bytes,
            frame_bytes * usize::from(frame_count.saturating_sub(1)),
        )?;
        let spawn_offset = SPAWN_TABLE + 4 * id;
        let palette = graphics::parse_palette(bank(front, 16, 512)?, 256)
            .map_err(|e| Error(e.to_string()))?;
        // These two templates disable palette uploads: BG0/BG3 display the
        // foreground's BG palette in both ordinary Leaf Forest acts.
        let leaf_backgrounds = if level <= 1 {
            let load = |index: usize| -> Result<Box<[u8]>, Error> {
                let h = header(TILEMAP_TABLE + 4 * index, 28)?;
                if (word(h, 0)?, word(h, 2)?) != (32, 30) {
                    return Err(Error("Invalid Leaf Forest text map".into()));
                }
                let tiles = graphics::decode_4bpp_tiles(bank(h, 8, long(h, 12)? as usize)?, 1024)
                    .map_err(|e| Error(e.to_string()))?;
                let layout = graphics::parse_tilemap(bank(h, 24, 1920)?, 32, 30, 960)
                    .map_err(|e| Error(e.to_string()))?;
                // Cache palette indices, not RGBA: checkpoint scripts animate BG palette RAM.
                let mut pixels = vec![0u8; 256 * 240];
                for (i, t) in layout.into_iter().enumerate() {
                    let tile = tiles
                        .get(usize::from(t.tile_index))
                        .ok_or_else(|| Error("Leaf Forest tile outside graphics".into()))?;
                    for y in 0..8 {
                        for x in 0..8 {
                            let color = tile[(if t.vertical_flip { 7 - y } else { y }) * 8
                                + (if t.horizontal_flip { 7 - x } else { x })];
                            pixels[(i / 32 * 8 + y) * 256 + (i % 32 * 8 + x)] = if color == 0 {
                                0
                            } else {
                                t.palette_bank * 16 + color
                            };
                        }
                    }
                }
                Ok(pixels.into_boxed_slice())
            };
            Some([load(113)?, load(3 * id + 2)?])
        } else {
            None
        };
        Ok(Self {
            level,
            spawn: (
                i32::from(word(bytes, spawn_offset)? as i16),
                i32::from(word(bytes, spawn_offset + 2)? as i16),
            ),
            width,
            height,
            map_width: usize::from(mw),
            maps,
            metatiles,
            tiles,
            animation,
            frame_bytes,
            frame_count,
            frame_delay,
            heights: bank(collision, 0, tile_count * 8)?,
            rotations: bank(collision, 4, tile_count)?,
            flags: bank(collision, 20, tile_count.div_ceil(8) * 2)?,
            palette,
            leaf_backgrounds,
        })
    }

    pub(crate) fn set_background_bank(&mut self, bank: u8, words: &[u16; 16]) -> Result<(), Error> {
        let start = usize::from(bank) * 16;
        let dest = self
            .palette
            .get_mut(start..start + 16)
            .ok_or_else(|| Error("BG palette bank outside hardware range".into()))?;
        for (dst, src) in dest.iter_mut().zip(words) {
            *dst = graphics::PaletteColor::from_word(*src);
        }
        Ok(())
    }

    pub fn tile_at(&self, x: i32, y: i32, layer: u8) -> Result<u16, Error> {
        if !(0..self.width).contains(&x) || !(0..self.height).contains(&y) || layer > 1 {
            return Err(Error(
                "Terrain coordinate/layer outside native stage".into(),
            ));
        }
        let tx = x as usize / 8;
        let ty = y as usize / 8;
        let mx = tx / METATILE_TILES;
        let my = ty / METATILE_TILES;
        let index = word(self.maps[layer as usize], (my * self.map_width + mx) * 2)? as usize;
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
        let x = x.clamp(0, self.width - 1);
        let y = y.clamp(0, self.height - 1);
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
        self.render_view_at(camera_x, camera_y, width, height, 0)
    }

    /// Render the foregrounds after this many UpdateBgAnimationTiles calls.
    pub fn render_view_at(
        &self,
        camera_x: i32,
        camera_y: i32,
        width: usize,
        height: usize,
        frame: u64,
    ) -> Result<Vec<u8>, Error> {
        self.render_layers([camera_x, camera_y], [width, height], frame, false, |_| {
            Ok(())
        })
    }

    /// Compose the represented native backgrounds and foreground; omit objects.
    /// A tick of zero is the initial (unscrolled) camera task.
    pub fn render_scene_at(
        &self,
        camera: [i32; 2],
        size: [usize; 2],
        tick: u64,
    ) -> Result<Vec<u8>, Error> {
        self.render_layers(camera, size, tick, true, |_| Ok(()))
    }

    /// OBJ priority 2 is ahead of BG2 on equal priority and behind BG1.
    pub(crate) fn render_layers(
        &self,
        [camera_x, camera_y]: [i32; 2],
        [width, height]: [usize; 2],
        frame: u64,
        background: bool,
        objects: impl FnOnce(&mut [u8]) -> Result<(), Error>,
    ) -> Result<Vec<u8>, Error> {
        let phase = if self.frame_count != 0 {
            ((frame / u64::from(self.frame_delay)) % u64::from(self.frame_count)) as usize
        } else {
            0
        };
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
        if let (true, Some([bg0, bg3])) = (background, &self.leaf_backgrounds) {
            let (cx, cy) = if frame == 0 {
                (0, 0)
            } else {
                (camera_x, camera_y)
            };
            for (i, pixel) in output.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let (sx, sy) = ((i % width) as i32, (i / width) as i32);
                let (far_x, far_y) = if (72..159).contains(&sy) {
                    (cx >> 7, cy >> 8)
                } else {
                    (0, 0)
                };
                for (map, dx, dy) in [(bg0, cx >> 6, cy >> 9), (bg3, far_x, far_y)] {
                    let x = (sx.wrapping_add(dx) & 255) as usize;
                    let y = (sy.wrapping_add(dy) & 255) as usize;
                    if let Some(&index) = map.get(y * 256 + x)
                        && index != 0
                    {
                        pixel[..3].copy_from_slice(&self.palette[usize::from(index)].rgb8());
                        break;
                    }
                }
            }
        }
        let draw_layer = |output: &mut [u8], layer: u8| -> Result<(), Error> {
            for sy in 0..height {
                let y = camera_y.saturating_add(sy as i32);
                if !(0..self.height).contains(&y) {
                    continue;
                }
                for sx in 0..width {
                    let x = camera_x.saturating_add(sx as i32);
                    if !(0..self.width).contains(&x) {
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
                    let offset = (tile as usize & 1023) * 32 + py * 4 + px / 2;
                    let byte = if phase != 0 && offset < self.frame_bytes {
                        self.animation[(phase - 1) * self.frame_bytes + offset]
                    } else {
                        self.tiles[offset]
                    };
                    let nibble = if px & 1 == 0 { byte & 15 } else { byte >> 4 };
                    if nibble == 0 {
                        continue;
                    }
                    let color = self.palette[(tile as usize >> 12) * 16 + nibble as usize].rgb8();
                    let at = (sy * width + sx) * 4;
                    output[at..at + 3].copy_from_slice(&color);
                }
            }
            Ok(())
        };
        draw_layer(&mut output, 1)?;
        objects(&mut output)?;
        draw_layer(&mut output, 0)?;
        Ok(output)
    }
}
