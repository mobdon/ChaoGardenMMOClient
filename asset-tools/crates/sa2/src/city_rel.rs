//! File-backed sections and relocation references of the pinned Steam stage modules.
use crate::{manifest, Error, Result};
use serde::Serialize;
use std::collections::BTreeMap;

pub const SHA256: &str = "bcb2f3e2dbead7bafebcda84063fba54bac831cbd9102fd0230d78a5b88b570b";
fn invalid() -> Error {
    Error::invalid("invalid or unsupported installed REL reference")
}

#[derive(Clone, Copy, Serialize)]
pub struct Section {
    pub file_offset: u32,
    pub size: u32,
    pub executable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Reference {
    pub module: u32,
    pub kind: u8,
    pub section: u8,
    pub addend: u32,
}

pub struct CityRel<'a> {
    bytes: &'a [u8],
    module: u32,
    sections: Vec<Section>,
    refs: BTreeMap<(u8, u32), Reference>,
}

impl<'a> CityRel<'a> {
    /// Distinct from the GSNE8P decompilation's expected disc build.
    pub fn read(bytes: &'a [u8]) -> Result<Self> {
        if bytes.len() != 3_304_912 || manifest::sha256(bytes) != SHA256 {
            return Err(Error::invalid("unsupported City Escape REL revision"));
        }
        Self::read_identified(bytes, 12)
    }
    /// Caller must first identify the released module by pinned digest.
    pub(crate) fn read_identified(bytes: &'a [u8], module: u32) -> Result<Self> {
        let word = |at: usize| -> Result<u32> {
            Ok(u32::from_be_bytes(
                bytes
                    .get(at..at + 4)
                    .ok_or_else(invalid)?
                    .try_into()
                    .unwrap(),
            ))
        };
        if word(0)? != module || word(28)? != 2 {
            return Err(invalid());
        }
        let dir = word(16)? as usize;
        let mut sections = Vec::new();
        for i in 0..word(12)? as usize {
            let flags = word(dir + i * 8)?;
            let section = Section {
                file_offset: flags & !1,
                size: word(dir + i * 8 + 4)?,
                executable: flags & 1 != 0,
            };
            if section.file_offset != 0
                && u64::from(section.file_offset) + u64::from(section.size) > bytes.len() as u64
            {
                return Err(invalid());
            }
            sections.push(section);
        }
        let mut refs = BTreeMap::new();
        let imports = word(40)? as usize;
        for off in (imports..imports + word(44)? as usize).step_by(8) {
            let imported_module = word(off)?;
            let mut stream = word(off + 4)? as usize;
            let mut src = None;
            let mut loc = 0u32;
            loop {
                let r = bytes.get(stream..stream + 8).ok_or_else(invalid)?;
                stream += 8;
                let (kind, section) = (r[2], r[3]);
                match kind {
                    203 => break, // END
                    202 => {
                        src = Some(section);
                        loc = 0;
                        continue;
                    } // SECTION
                    _ => {}
                }
                loc = loc
                    .checked_add(u16::from_be_bytes([r[0], r[1]]) as u32)
                    .ok_or_else(invalid)?;
                if kind == 201 {
                    continue;
                } // NOP advances the running offset
                let src = src.ok_or_else(invalid)?;
                let width = match kind {
                    1 | 10 => 4,
                    4 | 6 => 2,
                    _ => return Err(invalid()),
                };
                if loc.checked_add(width).ok_or_else(invalid)?
                    > sections.get(src as usize).ok_or_else(invalid)?.size
                {
                    return Err(invalid());
                }
                let addend = u32::from_be_bytes(r[4..].try_into().unwrap());
                if imported_module == module
                    && addend >= sections.get(section as usize).ok_or_else(invalid)?.size
                {
                    return Err(invalid());
                }
                if refs
                    .insert(
                        (src, loc),
                        Reference {
                            module: imported_module,
                            kind,
                            section,
                            addend,
                        },
                    )
                    .is_some()
                {
                    return Err(invalid());
                }
            }
        }
        Ok(Self {
            bytes,
            module,
            sections,
            refs,
        })
    }
    pub fn sections(&self) -> &[Section] {
        &self.sections
    }
    pub fn data(&self, section: u8) -> Result<&'a [u8]> {
        let s = self.sections.get(section as usize).ok_or_else(invalid)?;
        if s.file_offset == 0 {
            return Err(Error::invalid("REL section has no file data"));
        }
        Ok(&self.bytes[s.file_offset as usize..s.file_offset as usize + s.size as usize])
    }
    pub fn reference(&self, section: u8, offset: u32) -> Option<Reference> {
        self.refs.get(&(section, offset)).copied()
    }
    /// A missing relocation is a null pointer only if the source word is zero.
    /// Imported engine objects and instruction relocations are not local data pointers.
    pub fn local_pointer(&self, section: u8, offset: u32) -> Result<Option<(u8, u32)>> {
        let at = offset as usize;
        let word = self
            .data(section)?
            .get(at..at.checked_add(4).ok_or_else(invalid)?)
            .ok_or_else(invalid)?;
        match self.reference(section, offset) {
            Some(Reference {
                module,
                kind: 1,
                section,
                addend,
            }) if module == self.module => Ok(Some((section, addend))),
            None if word == [0; 4] => Ok(None),
            _ => Err(invalid()),
        }
    }
    /// Verify the paired HA/LO references used in the compiled resource selector.
    fn resource(&self, at: u32) -> Result<u32> {
        match (self.reference(1, at), self.reference(1, at + 4)) {
            (
                Some(Reference {
                    module: 12,
                    section: 5,
                    kind: 6,
                    addend: a,
                }),
                Some(Reference {
                    module: 12,
                    section: 5,
                    kind: 4,
                    addend: b,
                }),
            ) if a == b => Ok(a),
            _ => Err(invalid()),
        }
    }
    pub fn board_resources(&self) -> Result<Vec<BoardResources>> {
        let data = self.data(5)?;
        let local = |o| {
            self.local_pointer(5, o)?
                .filter(|(s, _)| *s == 5)
                .map(|(_, v)| v)
                .ok_or_else(invalid)
        };
        let mut result = Vec::new();
        for (variant, texture, model, other) in [
            ("single_player", 0x222, 0x23a, 0x22e),
            ("two_player_amy", 0x28a, 0x296, 0x2a2),
            ("two_player_metal_sonic", 0x2b2, 0x2be, 0x2ca),
            ("two_player_shadow", 0x2da, 0x2e6, 0x2f2),
            ("two_player_default", 0x302, 0x30e, 0x31a),
        ] {
            let texture = self.resource(texture)?;
            let names = local(texture)?;
            let i = texture as usize + 4;
            let count =
                u32::from_be_bytes(data.get(i..i + 4).ok_or_else(invalid)?.try_into().unwrap());
            if count > 256 {
                return Err(invalid());
            }
            let mut textures = Vec::new();
            for j in 0..count {
                let at = local(names + 12 * j)? as usize;
                let tail = data.get(at..).ok_or_else(invalid)?;
                let end = tail
                    .iter()
                    .take(256)
                    .position(|b| *b == 0)
                    .ok_or_else(invalid)?;
                textures.push(
                    std::str::from_utf8(&tail[..end])
                        .map_err(|_| invalid())?
                        .to_owned(),
                );
            }
            result.push(BoardResources {
                variant,
                texture_list: texture,
                model: self.resource(model)?,
                other_object: self.resource(other)?,
                textures,
            });
        }
        Ok(result)
    }
}

#[derive(Serialize)]
pub struct BoardResources {
    pub variant: &'static str,
    /// All three offsets are relative to data section 5.
    pub texture_list: u32,
    pub model: u32,
    pub other_object: u32,
    pub textures: Vec<String>,
}
