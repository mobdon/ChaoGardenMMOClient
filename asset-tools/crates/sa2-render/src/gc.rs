//! SA2B's GameCube land models (the "gc" attachments the sa2 crate keeps
//! opaque), read from the stage module with its relocations and loaded
//! with sa2-gx's vertex loader:
//!
//! - attachment: +0 vertex-attribute list, +8 opaque meshes, +0xC
//!   translucent meshes, +0x10 / +0x12 their counts, +0x14 center, +0x20
//!   radius;
//! - vertex attribute (0x10): attribute (1 position, 2 normal, 3 color 0,
//!   5 texcoord 0, 0xFF end), element size, element count, format word,
//!   data pointer, data size;
//! - mesh (0x10): parameter list, parameter count, display list, its size;
//! - parameter (8): type (high byte) and value. Type 1 is the vertex
//!   descriptor (GX VCD bits: two per attribute from bit 2, position
//!   first); type 8 the texture (id in the high half). Parameters carry
//!   over between a model's meshes.
//! - display list: GX draw commands, as GX reads them.
use crate::land::GxPart;
use sa2_asset_tools::city_rel::CityRel;
use sa2_gx::enums::{attr, comp_cnt, comp_type};
use sa2_gx::params::IDENTITY34;
use sa2_gx::vtx::{GxArray, Vcd, VtxAttrFmt, VtxDesc, VtxLoader, ATTR_COUNT};

/// The model's attribute kinds, as GX attributes, with the format SA2B
/// stores them in (picked by element size) and the VCD bits' shift.
fn attribute(kind: u8, size: u8) -> Option<(u32, VtxAttrFmt, u32)> {
    let f = |comp_cnt, comp_type, comp_shift| VtxAttrFmt {
        comp_type,
        comp_cnt,
        comp_shift,
    };
    Some(match (kind, size) {
        (1, 12) => (attr::POS, f(comp_cnt::POS_XYZ, comp_type::F32, 0), 2),
        (2, 12) => (attr::NRM, f(comp_cnt::NRM_XYZ, comp_type::F32, 0), 4),
        (3, 4) => (attr::CLR0, f(comp_cnt::CLR_RGBA, comp_type::RGBA8, 0), 6),
        // shortcut: 1/256 texcoords as the land uses; other sizes are
        // skipped until a model needs them.
        (5, 4) => (attr::TEX0, f(comp_cnt::TEX_ST, comp_type::S16, 8), 10),
        _ => return None,
    })
}

pub struct GcSource<'a> {
    pub rel: &'a CityRel<'a>,
    pub section: u8,
}

impl GcSource<'_> {
    fn data(&self) -> &[u8] {
        self.rel.data(self.section).unwrap_or(&[])
    }
    fn u8(&self, at: u32) -> u8 {
        self.data().get(at as usize).copied().unwrap_or(0)
    }
    fn u16(&self, at: u32) -> u16 {
        self.data()
            .get(at as usize..at as usize + 2)
            .map_or(0, |b| u16::from_be_bytes([b[0], b[1]]))
    }
    fn u32(&self, at: u32) -> u32 {
        self.data()
            .get(at as usize..at as usize + 4)
            .map_or(0, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn ptr(&self, at: u32) -> Option<u32> {
        match self.rel.local_pointer(self.section, at) {
            Ok(Some((s, off))) if s == self.section => Some(off),
            _ => None,
        }
    }
    fn bytes(&self, at: u32, len: u32) -> &[u8] {
        self.data()
            .get(at as usize..(at as usize).saturating_add(len as usize))
            .unwrap_or(&[])
    }

    /// The meshes of the model at `attach` (section offset), in model space.
    pub fn model(&self, attach: u32) -> Vec<GxPart> {
        let mut vat = [VtxAttrFmt::default(); ATTR_COUNT];
        let mut arrays: [GxArray<'_>; ATTR_COUNT] = Default::default();
        // (attribute, VCD shift) of every array the model has.
        let mut present = Vec::new();
        if let Some(mut a) = self.ptr(attach) {
            for _ in 0..16 {
                let kind = self.u8(a);
                if kind == 0xFF {
                    break;
                }
                if let (Some((at, fmt, shift)), Some(d)) =
                    (attribute(kind, self.u8(a + 1)), self.ptr(a + 8))
                {
                    vat[at as usize] = fmt;
                    arrays[at as usize] = GxArray {
                        data: self.bytes(d, self.u32(a + 0xC)),
                        stride: usize::from(self.u8(a + 1)),
                    };
                    present.push((at, shift));
                }
                a += 0x10;
            }
        }
        let mut parts = Vec::new();
        let mut vcd_bits = 0u32;
        let mut texture = None;
        for (list, count_at) in [(8u32, 0x10u32), (0xC, 0x12)] {
            let Some(meshes) = self.ptr(attach + list) else {
                continue;
            };
            for k in 0..u32::from(self.u16(attach + count_at)) {
                let me = meshes + 0x10 * k;
                if let Some(p) = self.ptr(me) {
                    for j in 0..self.u32(me + 4).min(64) {
                        let (t, v) = (self.u32(p + 8 * j) >> 24, self.u32(p + 8 * j + 4));
                        match t {
                            1 => vcd_bits = v,
                            8 => texture = Some((v >> 16) as u16),
                            _ => {}
                        }
                    }
                }
                let Some(dl) = self.ptr(me + 8) else { continue };
                let mut vcd: Vcd = [VtxDesc::default(); ATTR_COUNT];
                for &(at, shift) in &present {
                    vcd[at as usize].ty = (vcd_bits >> shift) & 3;
                }
                let loader = VtxLoader::new_multi([Some(vat); 8], vcd);
                let data = loader.run_vertices(self.bytes(dl, self.u32(me + 0xC)), &arrays);
                if data.indices.is_empty() {
                    continue;
                }
                parts.push(GxPart {
                    colors: vcd[attr::CLR0 as usize].ty != 0,
                    layout: loader.layout,
                    vertices: data.vertices,
                    indices: data.indices,
                    texture,
                    model: IDENTITY34,
                });
            }
        }
        parts
    }
}
