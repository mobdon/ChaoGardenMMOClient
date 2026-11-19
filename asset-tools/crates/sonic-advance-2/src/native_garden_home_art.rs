//! Frozen original Garden actors at native save positions.
//!
//! This is an artwork inspector. The record has no live Chao action/facing,
//! so the body uses original pose 0, facing 1, and a settled neutral orb.
//! Runtime AI, orb smoothing/bob and OAM ties between independent tasks are not
//! inferred from the save. No record or progression state is changed.
use crate::native_garden_art::{Scene, blit_clipped, descriptor_styled};
use crate::{Error, VerifiedRom, graphics::RenderedMap, native_shop_state::NativeShopFields};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct HomeArtReport {
    pub actors: Vec<HomeActor>,
    pub chao_pose: Option<u8>,
    pub chao_facing: Option<u8>,
    pub omitted_chao_appearance: Option<u8>,
}

/// Source-relative descriptor and source-selected OBJ tile/palette, together
/// with the original saved point and the native constructor's settled point.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct HomeActor {
    pub kind: &'static str,
    pub slot: u8,
    pub descriptor: usize,
    pub tile: u16,
    pub packed_palette_layer: u8,
    pub saved_position: [u8; 2],
    pub settled_position: [u8; 2],
    pub sprite_position: [i32; 2],
}

fn fail(e: impl std::fmt::Display) -> Error {
    Error(e.to_string())
}

/// The egg/fruit constructors clamp Y, then walk up one collision-map row.
/// The Chao constructor instead gives an unset X its native (88,104) point.
/// Invalid/underflowing positions fail before a caller's frame is changed.
fn settled(source: &[u8], mut point: [u8; 2], chao: bool) -> Result<[u8; 2], Error> {
    if chao && point[0] == 0 {
        point = [88, 104];
    } else if !chao {
        point[1] = point[1].min(159);
    }
    if point[0] >= 176 || point[1] >= 160 {
        return Err(fail(
            "Garden actor is outside the verified 22x20 native collision map",
        ));
    }
    for _ in 0..20 {
        let at = 0x21f34 + usize::from(point[1] / 8) * 22 + usize::from(point[0] / 8);
        if source
            .get(at)
            .copied()
            .ok_or_else(|| fail("Missing Garden collision map"))?
            == 0
        {
            return Ok(point);
        }
        point[1] = point[1]
            .checked_sub(8)
            .ok_or_else(|| fail("Garden actor has no bounded native settling point"))?;
    }
    Err(fail("Garden actor settling exceeded the native map height"))
}

fn chao_palette(record: &[u8]) -> usize {
    let appearance = usize::from(record[0x72]);
    if appearance > 11 {
        14
    } else if appearance != 0 {
        appearance
    } else {
        let mut form = usize::from(record[0x71]);
        if form > 18 {
            form = 1;
        }
        if form != 0 {
            form += 2;
        }
        let selected = form % 3;
        if selected == 0 { 0 } else { selected + 11 }
    }
}

fn parts<'a>(rom: &'a VerifiedRom<'_>, record: &[u8]) -> Result<(Scene<'a>, HomeArtReport), Error> {
    if record.len() < crate::native_hatching::RECORD_LEN {
        return Err(fail("Truncated native Garden record"));
    }
    let fields = NativeShopFields::read_raw_fields(record).map_err(fail)?;
    let source = rom.garden_bytes();
    let mut scene = Scene::new(source);
    scene
        .load_raw("garden-palette", 0x246e4, 512, "objPalette", 0)
        .map_err(fail)?;
    scene
        .load("garden-body", 0x24c24, "vram", 0x12d80)
        .map_err(fail)?;
    scene
        .load("garden-items", 0x268b4, "vram", 0x11580)
        .map_err(fail)?;
    let omitted_chao_appearance =
        (fields.chao_field_58 != 255 && record[0x72] > 11).then_some(record[0x72]);
    let has_chao = fields.chao_field_58 != 255 && omitted_chao_appearance.is_none();
    if has_chao {
        let palette = chao_palette(record);
        scene
            .load_raw("chao-palette", 0x248e4 + palette * 32, 32, "objPalette", 0)
            .map_err(fail)?;
    }
    let mut actors = Vec::new();
    if has_chao {
        let saved = [record[0x1d], record[0x1e]];
        let point = settled(source, saved, true)?;
        actors.push(HomeActor {
            kind: "chao-body",
            slot: 0,
            descriptor: 0x203b4,
            tile: 0x1a4,
            packed_palette_layer: 12,
            saved_position: saved,
            settled_position: point,
            sprite_position: point.map(i32::from),
        });
        let anchor = &source[0x20494..0x20496];
        actors.push(HomeActor {
            kind: "chao-neutral-orb",
            slot: 0,
            descriptor: 0x203fc,
            tile: 0x16c,
            packed_palette_layer: 12,
            saved_position: saved,
            settled_position: point,
            sprite_position: [
                i32::from(point[0]) + i32::from(anchor[0] as i8),
                i32::from(point[1]) + i32::from(anchor[1] as i8),
            ],
        });
    }
    for (slot, &id) in fields.fruit_ids.iter().enumerate() {
        if id == 255 {
            continue;
        }
        let saved = [
            fields.fruit_positions[slot].x,
            fields.fruit_positions[slot].y,
        ];
        let point = settled(source, saved, false)?;
        let bank = source[0x20efc + usize::from(id)];
        if bank >= 16 {
            return Err(fail("Invalid native fruit palette bank"));
        }
        actors.push(HomeActor {
            kind: "fruit",
            slot: slot as u8,
            descriptor: 0x205e4,
            tile: 0xbc + u16::from(id) * 4,
            packed_palette_layer: (bank << 4) | 12,
            saved_position: saved,
            settled_position: point,
            sprite_position: point.map(i32::from),
        });
    }
    if fields.egg_id != 255 {
        let id = usize::from(fields.egg_id);
        scene
            .load_raw("egg-palette", 0x24aa4 + id * 32, 32, "objPalette", 0x1c0)
            .map_err(fail)?;
        let saved = [fields.egg_position.x, fields.egg_position.y];
        let point = settled(source, saved, false)?;
        actors.push(HomeActor {
            kind: "egg",
            slot: 0,
            descriptor: 0x205e4,
            tile: if id == 0 { 0xd8 } else { 0xdc },
            packed_palette_layer: 0xec,
            saved_position: saved,
            settled_position: point,
            sprite_position: point.map(i32::from),
        });
    }
    // Native 0200117C orders ground actors by foot Y. A frozen save snapshot
    // supplies no task-list tie history; retain the documented insertion order.
    actors.sort_by_key(|actor| actor.settled_position[1]);
    Ok((
        scene,
        HomeArtReport {
            actors,
            chao_pose: has_chao.then_some(0),
            chao_facing: has_chao.then_some(1),
            omitted_chao_appearance,
        },
    ))
}

/// Overlay original actors on a 240x160 Garden picture. Source validation and
/// all actor decoding finish before the caller's picture is changed.
pub fn paint(
    rom: &VerifiedRom<'_>,
    record: &[u8],
    frame: &mut RenderedMap,
) -> Result<HomeArtReport, Error> {
    if frame.width != 240 || frame.height != 160 || frame.rgba.len() != 240 * 160 * 4 {
        return Err(fail("Garden actor canvas must be 240x160 RGBA"));
    }
    let (scene, report) = parts(rom, record)?;
    let mut images = Vec::with_capacity(report.actors.len());
    for actor in &report.actors {
        let (image, anchor, affine) = descriptor_styled(
            &scene,
            actor.descriptor,
            actor.tile,
            [0, 0, actor.packed_palette_layer],
        )
        .map_err(fail)?;
        if affine {
            return Err(fail("Unexpected affine Garden inspection actor"));
        }
        images.push((
            image,
            [
                actor.sprite_position[0] + anchor[0],
                actor.sprite_position[1] + anchor[1],
            ],
        ));
    }
    for (image, point) in images {
        blit_clipped(frame, &image, point[0], point[1]);
    }
    Ok(report)
}
