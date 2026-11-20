//! SA2B's GameCube land models (the "gc" attachments the sa2 crate keeps
//! opaque), read from the stage module with its relocations:
//!
//! - attachment: +0 vertex-attribute list, +8 opaque meshes, +0xC
//!   translucent meshes, +0x10 / +0x12 their counts, +0x14 center, +0x20
//!   radius;
//! - vertex attribute (0x10): attribute (1 position, 2 normal, 3 color 0,
//!   5 texcoord 0, 0xFF end), element size, element count, format word,
//!   data pointer, data size;
//! - mesh (0x10): parameter list, parameter count, display list, its size;
//! - parameter (8): type (high byte) and value. Type 1 is the index
//!   attribute flags (8- or 16-bit index per attribute, see [`IDX`]); type
//!   8 the texture (id in the high half). Parameters carry over between a
//!   model's meshes.
//! - display list: GX primitive opcode (0x80 quads, 0x90 triangles, 0x98
//!   strip, 0xA0 fan), big-endian vertex count, then per vertex the indices
//!   of each present attribute in GX order (position, normal, color,
//!   texcoord); an opcode of 0 ends it.
use crate::land::{Mesh, Vertex};
use sa2_asset_tools::city_rel::CityRel;

/// Index attribute flags: (present bit, 16-bit bit) for position, normal,
/// color 0, texcoord 0.
const IDX: [(u32, u32); 4] = [(0x8, 0x4), (0x20, 0x10), (0x80, 0x40), (0x800, 0x400)];

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
    fn f32(&self, at: u32) -> f32 {
        f32::from_bits(self.u32(at))
    }
    fn ptr(&self, at: u32) -> Option<u32> {
        match self.rel.local_pointer(self.section, at) {
            Ok(Some((s, off))) if s == self.section => Some(off),
            _ => None,
        }
    }

    /// Emit the model at `attach` (section offset) under `m`.
    pub fn model(&self, attach: u32, m: &dyn Fn([f32; 3], f32) -> [f32; 3], mesh: &mut Mesh) {
        let mut pos = Vec::new();
        let mut nrm = Vec::new();
        let mut col = Vec::new();
        let mut uv = Vec::new();
        if let Some(mut a) = self.ptr(attach) {
            for _ in 0..16 {
                let kind = self.u8(a);
                if kind == 0xFF {
                    break;
                }
                let count = u32::from(self.u16(a + 2));
                let Some(d) = self.ptr(a + 8) else {
                    a += 0x10;
                    continue;
                };
                match kind {
                    1 => pos = (0..count).map(|i| m(self.v3(d + 12 * i), 1.0)).collect(),
                    2 => nrm = (0..count).map(|i| m(self.v3(d + 12 * i), 0.0)).collect(),
                    3 => {
                        col = (0..count)
                            .map(|i| {
                                let w = self.u32(d + 4 * i);
                                [w >> 24, w >> 16, w >> 8, w].map(|c| (c & 0xFF) as f32 / 255.0)
                            })
                            .collect()
                    }
                    5 => {
                        uv = (0..count)
                            .map(|i| {
                                let s = self.u16(d + 4 * i) as i16;
                                let t = self.u16(d + 4 * i + 2) as i16;
                                [f32::from(s) / 256.0, f32::from(t) / 256.0]
                            })
                            .collect()
                    }
                    _ => {}
                }
                a += 0x10;
            }
        }
        let mut flags = 0u32;
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
                            1 => flags = v,
                            8 => texture = Some((v >> 16) as u16),
                            _ => {}
                        }
                    }
                }
                let Some(dl) = self.ptr(me + 8) else { continue };
                let end = dl + self.u32(me + 0xC);
                self.display_list(dl, end, flags, texture, (&pos, &nrm, &col, &uv), mesh);
            }
        }
    }

    fn v3(&self, at: u32) -> [f32; 3] {
        [self.f32(at), self.f32(at + 4), self.f32(at + 8)]
    }

    #[allow(clippy::type_complexity)]
    fn display_list(
        &self,
        mut at: u32,
        end: u32,
        flags: u32,
        texture: Option<u16>,
        arrays: (&[[f32; 3]], &[[f32; 3]], &[[f32; 4]], &[[f32; 2]]),
        mesh: &mut Mesh,
    ) {
        let (pos, nrm, col, uv) = arrays;
        let first = mesh.indices.len() as u32;
        while at + 3 <= end {
            let op = self.u8(at);
            if op == 0 {
                break;
            }
            let n = u32::from(self.u16(at + 1));
            at += 3;
            let mut verts = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let mut idx = [None; 4];
                for (slot, (has, wide)) in IDX.iter().enumerate() {
                    if flags & has != 0 {
                        if flags & wide != 0 {
                            idx[slot] = Some(self.u16(at) as usize);
                            at += 2;
                        } else {
                            idx[slot] = Some(usize::from(self.u8(at)));
                            at += 1;
                        }
                    }
                }
                let v = Vertex {
                    pos: idx[0].and_then(|i| pos.get(i)).copied().unwrap_or([0.0; 3]),
                    normal: idx[1]
                        .and_then(|i| nrm.get(i))
                        .copied()
                        .unwrap_or([0.0, 1.0, 0.0]),
                    uv: idx[3].and_then(|i| uv.get(i)).copied().unwrap_or([0.0; 2]),
                    color: idx[2].and_then(|i| col.get(i)).copied().unwrap_or([1.0; 4]),
                };
                verts.push(mesh.vertices.len() as u32);
                mesh.vertices.push(v);
            }
            match op & 0xF8 {
                0x90 => {
                    for t in verts.as_chunks::<3>().0 {
                        mesh.indices.extend(t);
                    }
                }
                0x98 => {
                    for i in 2..verts.len() {
                        let (a, b, c) = (verts[i - 2], verts[i - 1], verts[i]);
                        if i % 2 == 1 {
                            mesh.indices.extend([b, a, c]);
                        } else {
                            mesh.indices.extend([a, b, c]);
                        }
                    }
                }
                0xA0 => {
                    for i in 2..verts.len() {
                        mesh.indices.extend([verts[0], verts[i - 1], verts[i]]);
                    }
                }
                0x80 => {
                    for q in verts.as_chunks::<4>().0 {
                        mesh.indices.extend([q[0], q[1], q[2], q[0], q[2], q[3]]);
                    }
                }
                _ => break,
            }
        }
        let count = mesh.indices.len() as u32 - first;
        if count > 0 {
            mesh.draws.push(crate::land::Draw {
                texture,
                first,
                count,
            });
        }
    }
}
