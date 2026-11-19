//! Sonic vs Shadow 2 standalone platform collision. No landtable, movement,
//! world placement or inferred surface flags are added to the model.
use crate::{
    city_land,
    model::{self, DecodeBudget, Node},
    object_table,
    stage::{self, BasicAttachment},
    Error, Result,
};
use serde::Serialize;
pub const PLATFORM_RVA: u32 = 0x1251fa0;
#[derive(Debug, Serialize)]
pub struct Platform {
    pub node: Node,
    pub attachment: BasicAttachment,
}
pub(crate) fn decode(bytes: &[u8], root: u32) -> Result<Platform> {
    let mut budget = DecodeBudget::new();
    let mut nodes = model::read_nodes(bytes, root, bytes.len(), false, &mut budget)?;
    if nodes.len() != 1 {
        return Err(Error::invalid("expected one platform node"));
    }
    let node = nodes.pop().unwrap();
    if node.attach_offset == 0 {
        return Err(Error::invalid("expected a Basic platform attachment"));
    }
    let attachment = stage::basic(bytes, node.attach_offset, &mut budget)?;
    Ok(Platform { node, attachment })
}
/// Read the original local-space Basic model from the fingerprinted Steam EXE.
pub fn read(exe: &[u8]) -> Result<Platform> {
    let image = object_table::verify_image(exe)?;
    city_land::win32_basic_object(exe, &image, PLATFORM_RVA)
}
/// Read a standalone SA Tools version-3 Basic model of the same platform.
pub fn read_split(bytes: &[u8]) -> Result<Platform> {
    let r = model::Reader { data: bytes };
    if r.span(0, 8)? != b"SA1MDL\0\x03" {
        return Err(Error::invalid("expected a version 3 SA1MDL"));
    }
    let root = r.u32(8)?;
    let end = r.u32(12)? as usize;
    if end < 16 || end > bytes.len() {
        return Err(Error::invalid("invalid SA1MDL boundary"));
    }
    decode(&bytes[..end], root)
}
