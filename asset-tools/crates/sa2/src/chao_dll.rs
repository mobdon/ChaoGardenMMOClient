//! Chao models read straight from the Steam `Data_DLL.dll` image: the exported
//! `AL_RootObject` pointer array, nodes and Chunk attachments addressed by the
//! image's own virtual addresses. No DLL loading or execution (see `pe`).
use crate::model::{read_chunk_attachment_in, read_nodes_in, DecodeBudget, Model, Words};
use crate::motion::{read_motion_in, Motion};
use crate::pe::{read_pe, PeImage};
use crate::{Error, Result};
use std::collections::{BTreeMap, BTreeSet};

/// Little-endian reads where every address is a virtual address of the image.
struct ImageWords<'a> {
    data: &'a [u8],
    image: &'a PeImage,
}
impl Words for ImageWords<'_> {
    fn span(&self, at: usize, size: usize) -> Result<&[u8]> {
        let va = u32::try_from(at).map_err(|_| Error::invalid("address above 4 GiB"))?;
        let offset = self.image.va_to_offset(va, size)?;
        self.data
            .get(offset..offset + size)
            .ok_or_else(|| Error::invalid("image span outside the file"))
    }
    fn u16(&self, at: usize) -> Result<u16> {
        let b = self.span(at, 2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&self, at: usize) -> Result<u32> {
        let b = self.span(at, 4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

/// Every Chao model shares this 40-node tree; the motion table's clips are keyed to it.
pub const CHAO_NODES: u32 = 40;
/// `al_motion_table` length (motion numbers 0..=621).
pub const CHAO_MOTIONS: usize = 622;
/// One table row: motion pointer, mode, next motion, link step, start frame, end frame, play speed.
const MOTION_ROW: usize = 28;

/// Virtual address of element `index` (of `stride` bytes) of the exported array `name`.
fn export_slot(image: &PeImage, name: &str, index: usize, stride: usize) -> Result<usize> {
    let table = image
        .exports
        .iter()
        .find(|e| e.name == name && e.forwarded_to.is_none())
        .ok_or_else(|| Error::invalid(format!("{name} export not found")))?;
    index
        .checked_mul(stride)
        .and_then(|i| u32::try_from(i).ok())
        .and_then(|i| table.rva.checked_add(i))
        .and_then(|rva| rva.checked_add(image.image_base))
        .map(|va| va as usize)
        .ok_or_else(|| Error::invalid("exported array index overflow"))
}

/// A row of `al_motion_table` with its motion's keyframes (rotation keys are 16-bit).
#[derive(Debug)]
pub struct ChaoMotion {
    /// The motion's number in al_motion_table; `None` for a motion from outside the table,
    /// whose row values below are then empty.
    pub number: Option<usize>,
    /// Word 1 of the row, as stored (the game's motion mode).
    pub mode: u32,
    pub next: i32,
    pub link_step: i32,
    pub start_frame: f32,
    pub end_frame: f32,
    pub play_speed: f32,
    pub motion: Motion,
}

/// `al_motion_table[number]`.
pub fn read_chao_motion(dll: &[u8], number: usize) -> Result<ChaoMotion> {
    if number >= CHAO_MOTIONS {
        return Err(Error::invalid("Chao motion number outside the table"));
    }
    let image = read_pe(dll)?;
    let r = ImageWords {
        data: dll,
        image: &image,
    };
    let row = export_slot(&image, "al_motion_table", number, MOTION_ROW)?;
    Ok(ChaoMotion {
        number: Some(number),
        mode: r.u32(row + 4)?,
        next: r.u32(row + 8)? as i32,
        link_step: r.u32(row + 12)? as i32,
        start_frame: r.f32(row + 16)?,
        end_frame: r.f32(row + 20)?,
        play_speed: r.f32(row + 24)?,
        motion: read_motion_in(&r, r.u32(row)?, CHAO_NODES, true)?,
    })
}

/// `AL_RootObject[index]` as a model whose offsets are the image's virtual addresses.
pub fn read_chao_model(dll: &[u8], index: usize) -> Result<Model> {
    let image = read_pe(dll)?;
    let r = ImageWords {
        data: dll,
        image: &image,
    };
    let root_offset = r.u32(export_slot(&image, "AL_RootObject", index, 4)?)?;
    model_in(&r, root_offset)
}

/// The chunk model whose root node sits at virtual address `va` of PE image `file`
/// (for models the image does not export, by a traced address).
pub fn read_model_at(file: &[u8], va: u32) -> Result<Model> {
    let image = read_pe(file)?;
    model_in(
        &ImageWords {
            data: file,
            image: &image,
        },
        va,
    )
}

/// The motion at virtual address `va` of PE image `file`, for a model of `parts` animated nodes.
pub fn read_motion_at(
    file: &[u8],
    va: u32,
    parts: u32,
    short_rotation: bool,
) -> Result<ChaoMotion> {
    let image = read_pe(file)?;
    let r = ImageWords {
        data: file,
        image: &image,
    };
    Ok(ChaoMotion {
        number: None,
        mode: 0,
        next: 0,
        link_step: 0,
        start_frame: 0.,
        end_frame: 0.,
        play_speed: 0.,
        motion: read_motion_in(&r, va, parts, short_rotation)?,
    })
}

/// Steam `sonic2app.exe` addresses of the garden egg (SA Tools PC_SA2 ChaoCommon split list,
/// image base 0x400000 added): the one-piece egg and its 16-frame shake, the two-shell
/// hatching egg and its two hatch motions (27 and 20 frames).
pub const EGG: u32 = 0x125_D334;
pub const EGG_SHAKE: u32 = 0x125_C11C;
pub const HATCHING_EGG: u32 = 0x125_CC64;
pub const HATCH_PON: u32 = 0x125_C6C4;
pub const HATCH_KYORO: u32 = 0x125_CC9C;

fn model_in(r: &ImageWords, root_offset: u32) -> Result<Model> {
    let mut budget = DecodeBudget::new();
    let nodes = read_nodes_in(r, root_offset, false, &mut budget)?;
    let mut attachments = Vec::new();
    let mut seen = BTreeSet::new();
    for node in &nodes {
        if node.attach_offset != 0 && seen.insert(node.attach_offset) {
            attachments.push(read_chunk_attachment_in(
                r,
                node.attach_offset,
                &mut budget,
            )?);
        }
    }
    Ok(Model {
        format: "sa2-chunk".into(),
        version: 0,
        root_offset,
        metadata: Vec::new(),
        labels: BTreeMap::new(),
        nodes,
        attachments,
        unknowns: Vec::new(),
    })
}
