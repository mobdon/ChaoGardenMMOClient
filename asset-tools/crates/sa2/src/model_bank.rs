//! Character model banks (`*mdl.prs`, byte-identical on Steam and the GameCube disc).
//!
//! A decompressed bank starts with a directory of big-endian `(id: i32,
//! offset: u32)` rows ended by id -1. Each offset is a Ninja Chunk NJS_OBJECT
//! with file-relative pointers. Every field is big-endian at its own width:
//! node, attachment and vertex-chunk words are 32-bit, polygon-chunk fields
//! 16-bit. Established from the installed banks (2026-11-20): read this way,
//! every strip index lies inside the vertex range its attachment declares.
use crate::{
    archive,
    model::{self, BeReader, DecodeBudget, Model, Words},
    Error, Result,
};
use serde::Serialize;
use std::collections::BTreeSet;

const MAX_ROWS: usize = 4096;
const LIMIT: usize = 32 * 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BankModel {
    pub id: i32,
    pub model: Model,
}

pub fn read_prs(compressed: &[u8]) -> Result<Vec<BankModel>> {
    read(&archive::decompress_prs(compressed, LIMIT)?)
}

pub fn read(data: &[u8]) -> Result<Vec<BankModel>> {
    let r = BeReader { data };
    let mut rows = Vec::new();
    let mut end = None;
    for i in 0..=MAX_ROWS {
        let id = r.u32(i * 8)? as i32;
        if id == -1 {
            end = Some(i * 8 + 8);
            break;
        }
        rows.push((id, r.u32(i * 8 + 4)?));
    }
    let end = end.ok_or_else(|| Error::invalid("model bank directory has no -1 end"))?;
    let mut budget = DecodeBudget::new();
    let mut out = Vec::with_capacity(rows.len());
    for (id, root) in rows {
        if (root as usize) < end {
            return Err(Error::invalid("model bank entry points into its directory"));
        }
        let nodes = model::read_nodes_in(&r, root, false, &mut budget)?;
        let mut attachments = Vec::new();
        let mut seen = BTreeSet::new();
        for node in &nodes {
            if node.attach_offset != 0 && seen.insert(node.attach_offset) {
                attachments.push(model::read_chunk_attachment_in(
                    &r,
                    node.attach_offset,
                    &mut budget,
                )?);
            }
        }
        out.push(BankModel {
            id,
            model: Model {
                format: "sa2-chunk-big-endian".into(),
                version: 0,
                root_offset: root,
                metadata: Vec::new(),
                labels: Default::default(),
                nodes,
                attachments,
                unknowns: vec![
                    "Bank directory ids are kept as stored; which character action or part each id names is not established here".into(),
                ],
            },
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic bank: one directory row, a node with one attachment holding a
    /// 1-vertex chunk (type 34, 12-byte stride) and a 1-strip polygon chunk.
    fn bank() -> Vec<u8> {
        let mut b = vec![0u8; 0x100];
        let put32 =
            |b: &mut Vec<u8>, at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_be_bytes());
        let put16 =
            |b: &mut Vec<u8>, at: usize, v: u16| b[at..at + 2].copy_from_slice(&v.to_be_bytes());
        put32(&mut b, 0, 7);
        put32(&mut b, 4, 0x20);
        put32(&mut b, 8, u32::MAX);
        // node at 0x20: attach 0x60, unit scale
        put32(&mut b, 0x24, 0x60);
        for k in 0..3 {
            put32(&mut b, 0x40 + 4 * k, 1f32.to_bits());
        }
        // attachment at 0x60: vertex 0x80, polygon 0xa0, radius 1
        put32(&mut b, 0x60, 0x80);
        put32(&mut b, 0x64, 0xa0);
        put32(&mut b, 0x74, 1f32.to_bits());
        // vertex chunk: type 34, size (8 + 12) / 4 - 1 = 4 words, offset 0, count 1
        put32(&mut b, 0x80, 34 | 4 << 16);
        put32(&mut b, 0x84, 1 << 16);
        put32(&mut b, 0x88, 2f32.to_bits());
        put32(&mut b, 0x94, 255);
        // polygon: strip chunk type 64, size 2 u16s: 1 strip of length 1 (index 0); end
        put16(&mut b, 0xa0, 64);
        put16(&mut b, 0xa2, 3);
        put16(&mut b, 0xa4, 1);
        put16(&mut b, 0xa6, 1);
        put16(&mut b, 0xa8, 0);
        put16(&mut b, 0xaa, 255);
        b
    }

    #[test]
    fn reads_big_endian_chunk_models() {
        let models = read(&bank()).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, 7);
        let m = &models[0].model;
        assert_eq!(m.nodes.len(), 1);
        assert_eq!(m.attachments.len(), 1);
        let a = &m.attachments[0];
        assert_eq!(a.vertex_chunks[0].vertices[0].position, [2.0, 0.0, 0.0]);
        assert_eq!(a.polygon_chunks[0].kind, 64);
        assert_eq!(a.polygon_chunks[0].raw_words, [64, 3, 1, 1, 0]);
    }

    #[test]
    fn truncated_or_unterminated_banks_never_panic() {
        let b = bank();
        for n in 0..b.len() {
            assert!(read(&b[..n]).is_err() || n >= 0xac);
        }
        let mut no_end = b.clone();
        no_end[8..12].copy_from_slice(&0u32.to_be_bytes());
        assert!(read(&no_end).is_err());
        let mut into_dir = b;
        into_dir[4..8].copy_from_slice(&4u32.to_be_bytes());
        assert!(read(&into_dir).is_err());
    }
}
