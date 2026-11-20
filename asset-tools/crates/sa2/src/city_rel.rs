//! File-backed sections and relocation references of the pinned Steam stage modules.
use crate::{manifest, Error, Result};
use serde::Serialize;
use std::collections::BTreeMap;

pub const SHA256: &str = "bcb2f3e2dbead7bafebcda84063fba54bac831cbd9102fd0230d78a5b88b570b";
/// GameCube disc (GSNE8P) copies of stage RELs: (file name, SHA-256, module id).
/// The stage copies' sections match the Steam copies byte for byte; the boss
/// copies differ by the blocks in [`BOSS_INSERTS`].
pub const GC_RELS: &[(&str, &str, u32)] = &[
    (
        "stg13D.rel",
        "f5a986c2be0c7f8577538202cca0f714299617203a8cb1d7ef37d4d11f827423",
        13,
    ),
    (
        "stg16D.rel",
        "2bb313a3578036a98ebfa76351cdc5da8adb09f450fe62f37dc321a331a250c0",
        16,
    ),
    (
        "stg20D.rel",
        "ad76f467c63b903e5f635cee2c55e5dd0098c651bb473ba6791cebe7834f4a76",
        20,
    ),
    (
        "boss_bigfootD.rel",
        "84b7b7ca970495a9f7d246ea4fc23ba88297659211e677571b0fb39495fc6faa",
        82,
    ),
    (
        "boss_hotshotD.rel",
        "0989db26abb7559760ff7b49e6156c249b36b97211a18b59fc343e63ddcf460d",
        83,
    ),
];
/// Blocks only the disc copy of a boss REL has: (section, disc offset, length).
/// Each disc copy adds one 0x2F4-byte routine (placed after the `blr` ending the
/// preceding routine) with 8 bytes of constants and 0xC0 bytes of data; with
/// branches across the routine re-targeted, every other byte and relocation
/// matches the Steam copy (found by comparing the two copies).
pub const BOSS_INSERTS: &[(&str, [(u8, u32, u32); 3])] = &[
    (
        "boss_bigfootD.rel",
        [(1, 0x174bc, 0x2f4), (4, 0x990, 8), (5, 0x46418, 0xc0)],
    ),
    (
        "boss_hotshotD.rel",
        [(1, 0xf034, 0x2f4), (4, 0x840, 8), (5, 0x1e680, 0xc0)],
    ),
];
/// A boss REL in its disc layout: the disc copy as is, or the Steam copy with
/// the disc-only blocks left as zeros and every offset moved past them.
pub(crate) struct DiscLayout<'a> {
    source: &'a [u8],
    filename: &'static str,
    image: Option<(Vec<u8>, Vec<Section>)>,
}
impl<'a> DiscLayout<'a> {
    pub(crate) fn new(source: &'a [u8], filename: &'static str) -> Result<Self> {
        let inserts = Self::inserts(filename)?;
        let digest = manifest::sha256(source);
        if GC_RELS
            .iter()
            .any(|(f, h, _)| *f == filename && *h == digest)
        {
            return Ok(Self {
                source,
                filename,
                image: None,
            });
        }
        let steam = CityRel::read_stage(source, filename)?;
        let mut image = Vec::new();
        let mut sections = Vec::new();
        for (i, s) in steam.sections.iter().enumerate() {
            let insert = inserts.iter().find(|r| r.0 as usize == i);
            let size = s.size + insert.map_or(0, |r| r.2);
            if s.file_offset == 0 {
                sections.push(Section { size, ..*s });
                continue;
            }
            let mut data = steam.data(i as u8)?.to_vec();
            if let Some(&(_, at, len)) = insert {
                data.splice(
                    at as usize..at as usize,
                    std::iter::repeat_n(0, len as usize),
                );
            }
            if i == 1 {
                // Local branches carry no relocation; re-target the ones crossing the routine.
                for pc in (0..s.size).step_by(4) {
                    let at = shift(inserts, 1, pc) as usize;
                    let word = u32::from_be_bytes(data[at..at + 4].try_into().unwrap());
                    if word >> 26 != 18 || word & 2 != 0 || steam.reference(1, pc).is_some() {
                        continue;
                    }
                    let target = pc.wrapping_add((((word & 0x03ff_fffc) << 6) as i32 >> 6) as u32);
                    let moved = shift(inserts, 1, target).wrapping_sub(at as u32);
                    data[at..at + 4].copy_from_slice(
                        &((word & 0xfc00_0003) | (moved & 0x03ff_fffc)).to_be_bytes(),
                    );
                }
            }
            sections.push(Section {
                file_offset: image.len() as u32,
                size,
                ..*s
            });
            image.extend(data);
        }
        Ok(Self {
            source,
            filename,
            image: Some((image, sections)),
        })
    }
    fn inserts(filename: &str) -> Result<&'static [(u8, u32, u32)]> {
        BOSS_INSERTS
            .iter()
            .find(|(f, _)| *f == filename)
            .map(|(_, r)| r.as_slice())
            .ok_or_else(invalid)
    }
    pub(crate) fn rel(&self) -> Result<CityRel<'_>> {
        let Some((image, sections)) = &self.image else {
            return CityRel::read_stage(self.source, self.filename);
        };
        let inserts = Self::inserts(self.filename)?;
        let module = GC_RELS
            .iter()
            .find(|(f, _, _)| *f == self.filename)
            .ok_or_else(invalid)?
            .2;
        let steam = CityRel::read_stage(self.source, self.filename)?;
        let refs = steam
            .refs
            .iter()
            .map(|(&(s, at), &r)| {
                let r = if r.module == steam.module {
                    Reference {
                        module,
                        addend: shift(inserts, r.section, r.addend),
                        ..r
                    }
                } else {
                    r
                };
                ((s, shift(inserts, s, at)), r)
            })
            .collect();
        Ok(CityRel {
            bytes: image,
            module,
            sections: sections.clone(),
            refs,
        })
    }
}
/// A Steam-copy offset moved past the disc-only blocks before it.
fn shift(inserts: &[(u8, u32, u32)], section: u8, offset: u32) -> u32 {
    inserts
        .iter()
        .filter(|&&(s, at, _)| s == section && offset >= at)
        .fold(offset, |o, &(_, _, len)| o + len)
}
/// The Steam CartD.rel (module 80). Its disc copy differs beyond relocations
/// (see docs/FAN-STAGE.md), so the kart ports keep the Steam offsets and only
/// report engine imports at their GSNE8P addresses.
const CART_D_SHA256: &str = "71826645bae2a4064cd5ad4402facaf4da6c5caee0be7e19894ed7ceb2c2ee6c";
/// The GSNE8P main.dol address of each Steam stage REL import addend.
const GSNE8P_IMPORTS: &[(u32, u32)] = include!("gsne8p_imports.rs");
/// GSNE8P main.dol address for an engine address a Steam stage REL imports.
pub fn gsne8p_import(steam: u32) -> Option<u32> {
    GSNE8P_IMPORTS
        .binary_search_by_key(&steam, |&(s, _)| s)
        .ok()
        .map(|i| GSNE8P_IMPORTS[i].1)
}
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
    /// Stage-local player motion registrations; preserve source order and REL addresses.
    pub fn player_motion_bindings(&self) -> Result<serde_json::Value> {
        self.player_motion_report(false)
    }
    pub fn player_motion_keys(&self) -> Result<serde_json::Value> {
        self.player_motion_report(true)
    }
    fn player_motion_report(&self, keys: bool) -> Result<serde_json::Value> {
        let data = self.data(5)?;
        let mut rows = Vec::new();
        let start = 0x1fd2b0usize;
        for (n, row) in data
            .get(start..)
            .ok_or_else(invalid)?
            .chunks_exact(8)
            .enumerate()
        {
            let index = i16::from_be_bytes([row[0], row[1]]);
            if index == -1 {
                return Ok(serde_json::json!({"section":5,"tableOffset":start,"bindings":rows}));
            }
            if !(0..300).contains(&index) {
                return Err(invalid());
            }
            let (section, offset) = self
                .local_pointer(5, (start + n * 8 + 4) as u32)?
                .ok_or_else(invalid)?;
            let header = self
                .data(section)?
                .get(offset as usize..)
                .and_then(|tail| tail.get(..12))
                .ok_or_else(invalid)?;
            let (tracks_section, tracks_offset) =
                self.local_pointer(section, offset)?.ok_or_else(invalid)?;
            if tracks_offset as usize >= self.data(tracks_section)?.len() {
                return Err(invalid());
            }
            let mut binding = serde_json::json!({
                "index":index,"tag":i16::from_be_bytes([row[2],row[3]]),"section":section,"offset":offset,
                "frameCount":u32::from_be_bytes(header[4..8].try_into().unwrap()),
                "motionType":u16::from_be_bytes([header[8],header[9]]),
                "interpolationAndFactors":u16::from_be_bytes([header[10],header[11]]),
                "motionData":{"section":tracks_section,"offset":tracks_offset}
            });
            if keys {
                let count = u16::from_be_bytes([row[2], row[3]]) as u32;
                if header[8..12] != [0, 3, 0, 2]
                    || tracks_section != section
                    || tracks_offset.checked_add(count * 16) != Some(offset)
                {
                    return Err(invalid());
                }
                let mut nodes = Vec::new();
                for node in 0..count {
                    let addr = tracks_offset + node * 16;
                    let desc = &self.data(section)?[addr as usize..addr as usize + 16];
                    let mut channels = [Vec::new(), Vec::new()];
                    for (i, channel) in channels.iter_mut().enumerate() {
                        let n = u32::from_be_bytes(desc[8 + i * 4..12 + i * 4].try_into().unwrap());
                        let ptr = self.local_pointer(section, addr + i as u32 * 4)?;
                        if n == 0 && ptr.is_none() {
                            continue;
                        }
                        let (s, o) = ptr.ok_or_else(invalid)?;
                        let bytes = self
                            .data(s)?
                            .get(o as usize..)
                            .and_then(|tail| tail.get(..(n as usize).checked_mul(16)?))
                            .ok_or_else(invalid)?;
                        for k in bytes.chunks_exact(16) {
                            let word =
                                |j: usize| u32::from_be_bytes(k[j..j + 4].try_into().unwrap());
                            let xyz = [word(4), word(8), word(12)];
                            let value = if i == 0 {
                                let v = xyz.map(f32::from_bits);
                                if !v.iter().all(|x| x.is_finite()) {
                                    return Err(invalid());
                                }
                                serde_json::json!(v)
                            } else {
                                serde_json::json!(xyz.map(|x| x as i32))
                            };
                            channel.push(serde_json::json!([word(0), value]));
                        }
                    }
                    let [position, rotation] = channels;
                    nodes.push(serde_json::json!({"position":position,"rotation":rotation}));
                }
                binding["nodes"] = serde_json::json!(nodes);
            }
            rows.push(binding);
        }
        Err(invalid())
    }
    /// City Escape's stg13D.rel, Steam or GameCube disc copy.
    pub fn read(bytes: &'a [u8]) -> Result<Self> {
        Self::read_stage(bytes, "stg13D.rel")
            .map_err(|_| Error::invalid("unsupported City Escape REL revision"))
    }
    /// A stage REL by file name: its GameCube disc copy, or the installed Steam
    /// copy with every engine import moved to its GSNE8P main.dol address.
    pub(crate) fn read_stage(bytes: &'a [u8], filename: &str) -> Result<Self> {
        let digest = manifest::sha256(bytes);
        if let Some(&(_, _, module)) = GC_RELS
            .iter()
            .find(|(f, h, _)| *f == filename && *h == digest)
        {
            return Self::read_identified(bytes, module);
        }
        let module = crate::stage_land::catalog()
            .iter()
            .find(|i| i.filename == filename && i.size == bytes.len() && i.sha256 == digest)
            .map(|i| i.module)
            .or_else(|| (filename == "CartD.rel" && digest == CART_D_SHA256).then_some(80))
            .ok_or_else(|| Error::invalid("unsupported stage REL revision"))?;
        let mut rel = Self::read_identified(bytes, module)?;
        for r in rel.refs.values_mut().filter(|r| r.module == 0) {
            r.addend = gsne8p_import(r.addend).ok_or_else(invalid)?;
        }
        Ok(rel)
    }
    /// The module id this REL was read as.
    pub(crate) fn module_id(&self) -> u32 {
        self.module
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
                    module: m,
                    section: 5,
                    kind: 6,
                    addend: a,
                }),
                Some(Reference {
                    module: n,
                    section: 5,
                    kind: 4,
                    addend: b,
                }),
            ) if a == b && m == self.module && n == self.module => Ok(a),
            _ => Err(invalid()),
        }
    }
    /// ObjectCesobjExec, after range acceptance and optional executor/CCL dispatch.
    /// The coordinate is player zero after the engine's object-local transform.
    pub fn scenery_proximity(&self, player_local: Option<[f32; 3]>) -> Result<serde_json::Value> {
        let offset = self.resource(0xc1c6)? as usize;
        let data = self.data(5)?.get(offset..offset + 12).ok_or_else(invalid)?;
        let limits: [f32; 3] =
            std::array::from_fn(|i| f32::from_be_bytes(data[i * 4..i * 4 + 4].try_into().unwrap()));
        if player_local.is_some_and(|p| p.iter().any(|v| !v.is_finite())) {
            return Err(Error::invalid("nonfinite local player position"));
        }
        let inside =
            player_local.is_some_and(|p| [0, 2, 1].iter().all(|&i| p[i].abs() < limits[i]));
        Ok(
            serde_json::json!({"playerLocal":player_local,"halfExtents":limits,"mode":if inside {0}else{1},"dataSectionOffset":offset,
        "scope":"After range acceptance. No player gives mode one. No collision hit or range result inferred."}),
        )
    }
    /// Installed object-list wrappers referencing the shared ObjectCesobj initializer.
    pub fn scenery_descriptors(&self) -> Result<serde_json::Value> {
        let data = self.data(5)?;
        let word = |p: u32| -> Result<u32> {
            Ok(u32::from_be_bytes(
                data.get(p as usize..p as usize + 4)
                    .ok_or_else(invalid)?
                    .try_into()
                    .unwrap(),
            ))
        };
        let mut rows = Vec::new();
        for (entry, wrapper, load, delta) in
            [(0x1ffcfc, 0xbe9c, 0xbec2, 0), (0x1ffd0c, 0xbee8, 0xbf0e, 8)]
        {
            if self.local_pointer(5, entry)? != Some((1, wrapper)) {
                return Err(invalid());
            }
            let (section, name_at) = self.local_pointer(5, entry + 4)?.ok_or_else(invalid)?;
            let name = self
                .data(section)?
                .get(name_at as usize..)
                .ok_or_else(invalid)?
                .split(|b| *b == 0)
                .next()
                .ok_or_else(invalid)?;
            let name = std::str::from_utf8(name).map_err(|_| invalid())?;
            let header = self.resource(load)? + delta;
            let (5, start) = self.local_pointer(5, header)?.ok_or_else(invalid)? else {
                return Err(invalid());
            };
            let count = word(header + 4)?;
            if !(1..=256).contains(&count) {
                return Err(invalid());
            }
            let mut variants = Vec::new();
            for i in 0..count {
                let d = start + 40 * i;
                variants.push(serde_json::json!({"variant":i,"flags":word(d)?,"collision":self.local_pointer(5,d+12)?,"collisionCount":word(d+16)?,"displayCallback":self.local_pointer(5,d+20)?,"updateCallback":self.local_pointer(5,d+24)?}));
            }
            rows.push(
                serde_json::json!({"objectName":name,"wrapperOffset":wrapper,"variants":variants}),
            );
        }
        Ok(
            serde_json::json!({"objects":rows,"pointerBasis":"REL section and offset","selection":"low 8 bits of SET rotation X, capped at last variant"}),
        )
    }
    /// Normal SARROW display path (no alternate submode draw), after the task mode gate.
    pub fn arrow_display(&self, mode: i8, tick: u32) -> Result<serde_json::Value> {
        let local = |at| {
            self.local_pointer(5, at)?
                .filter(|(s, _)| *s == 5)
                .map(|(_, v)| v)
                .ok_or_else(invalid)
        };
        let header = self.resource(0xbec2)?;
        let descriptor = local(header)?;
        let model = local(descriptor + 8)?;
        let texlist = self.local_pointer(5, descriptor + 4)?;
        let alternate = self.local_pointer(5, descriptor + 0x1c)?;
        if alternate.is_some() {
            return Err(Error::invalid("unsupported alternate SARROW descriptor"));
        }
        let section = self.data(5)?;
        let root_geometry = self.local_pointer(5, model + 4)?;
        let mut rows = Vec::new();
        if mode == 0 {
            let mut node = local(model + 0x2c)?;
            let phase = tick % 90;
            let active = 2 - phase / 30;
            let age = phase % 30;
            let constants = self.data(4)?;
            let constant = |offset: usize| {
                f32::from_be_bytes(constants[offset..offset + 4].try_into().unwrap())
            };
            for index in 0..3 {
                let weight = if index == active {
                    if age < 8 {
                        age as f32 * constant(0x268)
                    } else {
                        constant(0x26c)
                    }
                } else if index == (active + 1) % 3 && age < 8 {
                    (8 - age) as f32 * constant(0x268)
                } else {
                    constant(0x270)
                };
                let translation = section
                    .get(node as usize + 8..node as usize + 20)
                    .ok_or_else(invalid)?;
                let translation: [f32; 3] = std::array::from_fn(|i| {
                    f32::from_be_bytes(translation[4 * i..4 * i + 4].try_into().unwrap())
                });
                rows.push(serde_json::json!({"index":index,"nodeOffset":node,"localTranslation":translation,"geometry":self.local_pointer(5,node+4)?,"weight":weight,"draw":weight>constant(0x274)}));
                if index < 2 {
                    node = local(node + 0x30)?;
                }
            }
        }
        Ok(
            serde_json::json!({"visibleMode":mode==0,"modelOffset":model,"textureList":texlist,"alternateDescriptor":alternate,"rootGeometry":root_geometry,"drawRootGeometry":mode==0 && root_geometry.is_some(),"children":rows,"scope":"Original SARROW descriptor has no alternate draw resource. Child translations are object-local; no pixel rendering."}),
        )
    }
    /// World-space evaluation for the cardinal orientations in the installed normal SET.
    pub fn placed_arrows(
        &self,
        layout: &crate::city_stage::CityLayout,
        player: Option<[f32; 3]>,
        tick: u32,
    ) -> Result<serde_json::Value> {
        let mut rows = Vec::new();
        for (slot, p) in layout.placements.iter().enumerate() {
            if layout.resolve(slot)?.1.name != "SARROW" {
                continue;
            }
            let yaw = p.rotation[1];
            if yaw % 16384 != 0 {
                return Err(Error::invalid("unverified noncardinal arrow yaw"));
            }
            let local = player.map(|pos| {
                let [x, y, z] = std::array::from_fn(|i| pos[i] - p.position[i]);
                match yaw / 16384 {
                    0 => [x, y, z],
                    1 => [-z, y, x],
                    2 => [-x, y, -z],
                    _ => [z, y, -x],
                }
            });
            let proximity = self.scenery_proximity(local)?;
            let mode = proximity["mode"].as_i64().ok_or_else(invalid)? as i8;
            let display = self.arrow_display(mode, tick)?;
            rows.push(serde_json::json!({"slot":slot,"position":p.position,"yaw":yaw,"proximity":proximity,"display":display}));
        }
        Ok(
            serde_json::json!({"sourceLayer":layout.source_layer,"frameCounter":tick,"playerWorld":player,"arrows":rows,"scope":"Evaluated after range acceptance, using original SET poses. No task scheduling or renderer."}),
        )
    }
    /// Verify the three immediate arguments and external target at each board input call.
    pub fn board_input_handoffs(&self) -> Result<serde_json::Value> {
        let code = self.data(1)?;
        let mut sites = Vec::new();
        for at in [0xa04u32, 0xa20, 0xa98, 0xab4] {
            let reference = self
                .reference(1, at)
                .filter(|r| r.module == 0 && r.kind == 10 && r.addend == 0x8003a508)
                .ok_or_else(invalid)?;
            let fetch = |p: u32| -> Result<u32> {
                Ok(u32::from_be_bytes(
                    code.get(p as usize..p as usize + 4)
                        .ok_or_else(invalid)?
                        .try_into()
                        .unwrap(),
                ))
            };
            if fetch(at)? != 0x48000001 {
                return Err(invalid());
            }
            let mut args = [0i32; 3];
            for (i, arg) in args.iter_mut().enumerate() {
                let op = fetch(at - 12 + 4 * i as u32)?;
                if op >> 16 != 0x3800 + 32 * (3 + i as u32) {
                    return Err(invalid());
                }
                *arg = i32::from(op as i16);
            }
            sites.push(serde_json::json!({"codeSectionOffset":at,"target":reference.addend,"arguments":args}));
        }
        Ok(
            serde_json::json!({"callee":"SetInputP","callSites":sites,"scope":"Imported calls only; does not identify the resulting player mode."}),
        )
    }
    fn texture_names(&self, texture: u32) -> Result<Vec<String>> {
        let data = self.data(5)?;
        let local = |o| {
            self.local_pointer(5, o)?
                .filter(|(s, _)| *s == 5)
                .map(|(_, v)| v)
                .ok_or_else(invalid)
        };
        let names = local(texture)?;
        let i = texture as usize + 4;
        let count = u32::from_be_bytes(data.get(i..i + 4).ok_or_else(invalid)?.try_into().unwrap());
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
        Ok(textures)
    }
    fn board_nodes(&self, root: u32) -> Result<Vec<BoardNode>> {
        // Ninja motion channels follow child-before-sibling traversal.
        let mut pending = vec![(root, None)];
        let mut seen = std::collections::BTreeSet::new();
        let mut nodes = Vec::new();
        while let Some((offset, parent)) = pending.pop() {
            if !seen.insert(offset) {
                return Err(invalid());
            }
            let bytes = self
                .data(5)?
                .get(offset as usize..)
                .and_then(|tail| tail.get(..52))
                .ok_or_else(invalid)?;
            let word = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
            let vector = |at: usize| [word(at), word(at + 4), word(at + 8)];
            let position = vector(8).map(f32::from_bits);
            let scale = vector(32).map(f32::from_bits);
            if !position.iter().chain(&scale).all(|v| v.is_finite()) {
                return Err(invalid());
            }
            let idx = nodes.len();
            for (field, up) in [(48, parent), (44, Some(idx))] {
                if let Some((section, next)) = self.local_pointer(5, offset + field)? {
                    if section != 5 {
                        return Err(invalid());
                    }
                    pending.push((next, up));
                }
            }
            nodes.push(BoardNode {
                offset,
                parent,
                flags: word(0),
                position,
                rotation: vector(20).map(|v| v as i32),
                scale,
                geometry: self.local_pointer(5, offset + 4)?,
            });
        }
        Ok(nodes)
    }
    pub fn board_resources(&self) -> Result<Vec<BoardResources>> {
        let mut result = Vec::new();
        for (variant, texture, model, other) in [
            ("single_player", 0x222, 0x23a, 0x22e),
            ("two_player_amy", 0x28a, 0x296, 0x2a2),
            ("two_player_metal_sonic", 0x2b2, 0x2be, 0x2ca),
            ("two_player_shadow", 0x2da, 0x2e6, 0x2f2),
            ("two_player_default", 0x302, 0x30e, 0x31a),
        ] {
            let texture = self.resource(texture)?;
            let textures = self.texture_names(texture)?;
            let model = self.resource(model)?;
            let other_object = self.resource(other)?;
            result.push(BoardResources {
                variant,
                texture_list: texture,
                model,
                other_object,
                model_nodes: self.board_nodes(model)?,
                other_nodes: self.board_nodes(other_object)?,
                textures,
            });
        }
        Ok(result)
    }
}

#[derive(Serialize)]
pub struct BoardNode {
    pub offset: u32,
    pub parent: Option<usize>,
    pub flags: u32,
    pub position: [f32; 3],
    pub rotation: [i32; 3],
    pub scale: [f32; 3],
    pub geometry: Option<(u8, u32)>,
}

#[derive(Serialize)]
pub struct BoardResources {
    pub model_nodes: Vec<BoardNode>,
    pub other_nodes: Vec<BoardNode>,
    pub variant: &'static str,
    /// All three offsets are relative to data section 5.
    pub texture_list: u32,
    pub model: u32,
    pub other_object: u32,
    pub textures: Vec<String>,
}
/// Recover the descriptor passed by the standard stage registration prolog.
/// Offsets refer to REL sections, never process addresses in the Rust host.
pub fn stage_registration(bytes: &[u8]) -> Result<serde_json::Value> {
    let digest = manifest::sha256(bytes);
    // Stage 42 has a registration descriptor but no mapped terrain in the land catalog.
    let module = if bytes.len() == 345422
        && digest == "51d7b743c95cc8533e402533d73d43abab04c4de2e08770d615e143e1925385d"
    {
        41
    } else {
        crate::stage_land::catalog()
            .iter()
            .find(|s| s.filename.ends_with(".rel") && s.size == bytes.len() && s.sha256 == digest)
            .ok_or_else(invalid)?
            .module
    };
    let rel = CityRel::read_identified(bytes, module)?;
    let entry = u32::from_be_bytes(bytes[52..56].try_into().unwrap());
    if bytes[48] != 1 {
        return Err(invalid());
    }
    let code = rel.data(1)?;
    let body = code
        .get(entry as usize..)
        .and_then(|b| b.get(..40))
        .ok_or_else(invalid)?;
    for (word, expected) in body.as_chunks::<4>().0.iter().zip([
        0x7c0802a6, 0x90010004, 0x9421fff8, 0x3c600000, 0x38630000, 0x48000001, 0x8001000c,
        0x38210008, 0x7c0803a6, 0x4e800020,
    ]) {
        if u32::from_be_bytes(*word) != expected {
            return Err(invalid());
        }
    }
    let hi = rel.reference(1, entry + 14).ok_or_else(invalid)?;
    let lo = rel.reference(1, entry + 18).ok_or_else(invalid)?;
    let call = rel.reference(1, entry + 20).ok_or_else(invalid)?;
    if (hi.module, hi.section, hi.kind) != (module, 5, 6)
        || lo != (Reference { kind: 4, ..hi })
        || (call.module, call.kind, call.addend) != (0, 10, 0x8001a01c)
    {
        return Err(invalid());
    }
    let descriptor = hi.addend;
    let data = rel.data(5)?;
    let label = data
        .get(descriptor as usize..)
        .and_then(|b| b.get(..12))
        .ok_or_else(invalid)?;
    let end = label.iter().position(|b| *b == 0).ok_or_else(invalid)?;
    let name = std::str::from_utf8(&label[..end]).map_err(|_| invalid())?;
    let callback = |delta| -> Result<Option<u32>> {
        match rel.local_pointer(5, descriptor + delta)? {
            Some((1, off)) if off % 4 == 0 && (off as usize) < code.len() => Ok(Some(off)),
            None => Ok(None),
            _ => Err(invalid()),
        }
    };
    let mut registration = serde_json::json!({"module":module,"prolog":entry,"descriptor":descriptor,"name":name,
        "registerTarget":call.addend,"startup":callback(12)?,"teardown":callback(16)?,"entry":callback(20)?});
    if module == 82 {
        // Section 1:6f64 assembles a stack vector for the direct call at 6fa8.
        // The SHA-256 gate above pins the instructions and register assignments.
        let pair = |at, section| -> Result<u32> {
            let hi = rel.reference(1, at).ok_or_else(invalid)?;
            let lo = rel.reference(1, at + 4).ok_or_else(invalid)?;
            if (hi.module, hi.section, hi.kind) != (module, section, 6)
                || lo != (Reference { kind: 4, ..hi })
            {
                return Err(invalid());
            }
            Ok(hi.addend)
        };
        let scalar = |at| -> Result<f32> {
            let off = pair(at, 4)? as usize;
            Ok(f32::from_be_bytes(
                rel.data(4)?
                    .get(off..off + 4)
                    .ok_or_else(invalid)?
                    .try_into()
                    .unwrap(),
            ))
        };
        let x = scalar(0x6f7a)?;
        let yz = scalar(0x6f8a)?;
        registration["bossTaskSetup"] = serde_json::json!({
            "constructorSection1Offset":0, "position":[x,yz,yz], "yaw":-16384,
            "nextTaskSection1Offset":pair(0x6fae, 1)?
        });
        // The executor indexes a relocated jump table with the signed mode byte.
        // Four entries jump directly to the common continuation.
        let table = pair(0x5bd2, 5)?;
        let mut handlers = Vec::new();
        for index in 0..16 {
            let (1, arm) = rel
                .local_pointer(5, table + index * 4)?
                .ok_or_else(invalid)?
            else {
                return Err(invalid());
            };
            if arm == 0x5c74 {
                handlers.push(None);
                continue;
            }
            let pc = arm as usize + 4;
            let branch = u32::from_be_bytes(
                code.get(pc..pc + 4)
                    .ok_or_else(invalid)?
                    .try_into()
                    .unwrap(),
            );
            if branch & 0xfc00_0003 != 0x4800_0001 {
                return Err(invalid());
            }
            let displacement = ((branch << 6) as i32 >> 6) & !3;
            let target = (pc as u32)
                .checked_add_signed(displacement)
                .ok_or_else(invalid)?;
            if target as usize >= code.len() {
                return Err(invalid());
            }
            handlers.push(Some(target));
        }
        registration["modeHandlerSection1Offsets"] = serde_json::json!(handlers);
        // Intro routine 1:c80, submode zero: three scalar stores to slot zero.
        let v = scalar(0xd42)?;
        registration["introPlayerWrite"] = serde_json::json!({"slot":0,"position":[v,v,v]});
    }
    Ok(registration)
}

/// Run Hot Shot's slot-zero stage constraint (section 1:7050 -> 6fcc).
pub fn hot_shot_arena(bytes: &[u8], mut position: [f32; 3]) -> Result<[f32; 3]> {
    let layout = DiscLayout::new(bytes, "boss_hotshotD.rel")?;
    if !position.iter().all(|v| v.is_finite()) {
        return Err(Error::invalid("expected finite player coordinates"));
    }
    let rel = layout.rel()?;
    let bound = |offset| -> Result<f32> {
        let hi = rel.reference(1, offset).ok_or_else(invalid)?;
        if (hi.module, hi.section, hi.kind) != (rel.module_id(), 4, 6)
            || rel.reference(1, offset + 4) != Some(Reference { kind: 4, ..hi })
        {
            return Err(invalid());
        }
        let start = hi.addend as usize;
        Ok(f32::from_be_bytes(
            rel.data(4)?
                .get(start..start + 4)
                .ok_or_else(invalid)?
                .try_into()
                .unwrap(),
        ))
    };
    for (axis, lo, hi) in [(0, 0x6ffe, 0x6fe2), (2, 0x7036, 0x701a)] {
        position[axis] = position[axis].clamp(bound(lo)?, bound(hi)?);
    }
    Ok(position)
}

/// State after N consecutive intro callbacks beginning in submode zero.
/// Engine calls are reported at their call sites; their effects are not simulated.
pub fn hot_shot_intro_clock(bytes: &[u8], updates: u16) -> Result<serde_json::Value> {
    let layout = DiscLayout::new(bytes, "boss_hotshotD.rel")?;
    let rel = layout.rel()?;
    let code = rel.data(1)?;
    let immediate = |at: usize| u16::from_be_bytes([code[at + 2], code[at + 3]]);
    let initial = immediate(0xdb8);
    if updates == 0 || updates > initial + 1 {
        return Err(Error::invalid(
            "expected 1 through 451 consecutive intro updates",
        ));
    }
    let timer = initial - (updates - 1);
    let exited = timer == 0;
    Ok(serde_json::json!({
        "updates":updates, "mode":if exited {2} else {1},
        "submode":if exited {0} else {1}, "timer":timer,
        "callsEngine8006adfc":timer < immediate(0xeb8) && timer > immediate(0xec0),
        "entersNextMode":exited
    }))
}

/// Replay the submode-specific opening of mode two, stopping before the shared tail.
pub fn hot_shot_mode_two_opening(
    bytes: &[u8],
    polls: u16,
    ready_on_last: bool,
) -> Result<serde_json::Value> {
    let layout = DiscLayout::new(bytes, "boss_hotshotD.rel")?;
    if ready_on_last && polls == 0 {
        return Err(Error::invalid(
            "readiness requires at least one submode-one poll",
        ));
    }
    let rel = layout.rel()?;
    let scalar = |at| hot_shot_scalar(&rel, at);
    let mut amount = scalar(0x1142)?;
    let step = scalar(0x11b2)?;
    let cap = scalar(0x11ca)?;
    for _ in 0..polls {
        amount = (amount + step).min(cap);
    }
    Ok(serde_json::json!({
        "mode":2,"submode":if ready_on_last {2} else {1},
        "timer":0u16.wrapping_sub(polls), "workB8":amount,
        "entryWork58":scalar(0x1132)?, "entryRequest":8,
        "lastLocal790Vector":(polls != 0).then_some([amount;3]),
        "readyBranchWork10Write":if ready_on_last {Some(scalar(0x120e)?)} else {None},
        "scope":"Submode-specific writes before the common tail; engine readiness is supplied and helper effects are not simulated."
    }))
}

fn hot_shot_scalar(rel: &CityRel, at: u32) -> Result<f32> {
    let data = rel.data(4)?;
    let hi = rel.reference(1, at).ok_or_else(invalid)?;
    if (hi.module, hi.section, hi.kind) != (rel.module_id(), 4, 6)
        || rel.reference(1, at + 4) != Some(Reference { kind: 4, ..hi })
    {
        return Err(invalid());
    }
    let offset = hi.addend as usize;
    Ok(f32::from_be_bytes(
        data.get(offset..offset + 4)
            .ok_or_else(invalid)?
            .try_into()
            .unwrap(),
    ))
}

/// One mode-two, submode-two tick. External readiness is observed only when queried.
pub fn hot_shot_mode_two_step(bytes: &[u8], request: &[u8]) -> Result<serde_json::Value> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct State {
        work04: f32,
        work10: f32,
        work_bc: u8,
        flags: u16,
        controller_state: i32,
        ready: Option<bool>,
    }
    let s: State = serde_json::from_slice(request)?;
    if s.ready.is_some() != (s.controller_state == 1)
        || s.work_bc >= 15
        || !s.work04.is_finite()
        || !s.work10.is_finite()
    {
        return Err(Error::invalid(
            "expected finite work fields, counter 0..14 and readiness only for controller state 1",
        ));
    }
    let layout = DiscLayout::new(bytes, "boss_hotshotD.rel")?;
    let rel = layout.rel()?;
    let scalar = |at| hot_shot_scalar(&rel, at);
    let request_two = s.ready == Some(true);
    let flags = if request_two { s.flags & !1 } else { s.flags };
    let mut work10 = s.work10 * scalar(0x12b6)? + scalar(0x12ce)?;
    let work04 = s.work04 * scalar(0x12e6)?;
    if flags & 1 == 0 {
        work10 += scalar(0x15be)?;
    }
    let pair = s.work_bc > 0 && s.work_bc % 5 == 0;
    let next = if s.work_bc == 0 || s.work_bc == 14 {
        0
    } else {
        s.work_bc + 1
    };
    Ok(
        serde_json::json!({"mode":2,"submode":if work04<0. {3} else {2},
        "work04":work04,"work10":work10,"workBC":next,"flags":flags,
        "requestedControllerState":request_two.then_some(2),"local51ee0Calls":if pair {2} else {0},
        "scope":"Scalar and control writes; helper effects and subsequent boss executor work are excluded."}),
    )
}

/// Mode 2 submode 4 writes through the jump to the shared tail at 1:15a8.
pub fn hot_shot_mode_two_exit(bytes: &[u8], request: &[u8]) -> Result<serde_json::Value> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct State {
        x: f32,
        private28: f32,
        work_angles: [u32; 3],
        flags: u16,
        timer: u16,
        ready: bool,
    }
    let s: State = serde_json::from_slice(request)?;
    if !s.x.is_finite() || !s.private28.is_finite() {
        return Err(Error::invalid("expected finite boss state"));
    }
    let layout = DiscLayout::new(bytes, "boss_hotshotD.rel")?;
    let rel = layout.rel()?;
    let scalar = |at| hot_shot_scalar(&rel, at);
    let mode = if !s.ready {
        2
    } else if s.private28 < scalar(0x1542)? {
        9
    } else {
        3
    };
    let angles = [0x210, 0x250, 0x300]
        .into_iter()
        .zip(s.work_angles)
        .map(|(inc, v)| v.wrapping_add(inc))
        .collect::<Vec<_>>();
    Ok(
        serde_json::json!({"mode":mode,"submode":if s.ready {0} else {4},
        "workAngles":angles,"work00":if s.x>scalar(0x14ea)? {scalar(0x14fe)?} else {scalar(0x1512)?},
        "flags":if s.ready {s.flags & !0x200} else {s.flags},
        "timer":if s.ready {0} else {s.timer}, "workByte3Write":s.ready.then_some(0),
        "privateByte2Write":(mode==3).then_some(2),"requestedControllerState":s.ready.then_some(3),
        "scope":"Submode four writes before shared tail; readiness is supplied, helper effects are not simulated."}),
    )
}

/// Mode 2 submode 3, up to the shared tail, using the position observed after helper calls.
pub fn hot_shot_settle(bytes: &[u8], request: &[u8]) -> Result<serde_json::Value> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct State {
        y: f32,
        work04: f32,
        work10: f32,
        angles: [u32; 2],
        flags: u16,
        timer: u16,
        controller_state: i32,
        ready: Option<bool>,
    }
    let s: State = serde_json::from_slice(request)?;
    if ![s.y, s.work04, s.work10].iter().all(|v| v.is_finite())
        || (s.controller_state == 1) != s.ready.is_some()
    {
        return Err(Error::invalid(
            "expected finite state and readiness only for controller state one",
        ));
    }
    let layout = DiscLayout::new(bytes, "boss_hotshotD.rel")?;
    let rel = layout.rel()?;
    let c = |at| hot_shot_scalar(&rel, at);
    let target = c(0x13ce)?;
    let delta = (s.y - target).clamp(c(0x1412)?, c(0x13f6)?);
    let damped = s.work04 * c(0x13e2)?;
    let corrected = s.work10 - c(0x142e)? * delta;
    let (lo, hi) = (c(0x1446)?, c(0x145a)?);
    let settled = delta > lo && delta < hi && damped > lo && damped < hi;
    let ready = s.ready == Some(true);
    Ok(
        serde_json::json!({"mode":2,"submode":if settled {4} else {3},
        "timer":if settled {30} else {s.timer},"y":if settled {target} else {s.y},
        "work04":if settled {0.} else {damped},"work10":if settled {0.} else {corrected},
        "angles":[s.angles[0].wrapping_add(0x210),s.angles[1].wrapping_add(0x250)],
        "flags":if ready {s.flags & !1} else {s.flags},"requestedControllerState":ready.then_some(2),
        "scope":"Submode-three writes before shared tail; position is observed after helpers."}),
    )
}

/// Control, timer and common arithmetic of executor mode 9 (section 1:4ae0).
pub fn hot_shot_mode_nine(bytes: &[u8], request: &[u8]) -> Result<serde_json::Value> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct State {
        submode: u8,
        timer: u16,
        remaining: u8,
        private28: f32,
        flags: u16,
        collision_flags: u32,
        work10: f32,
        angles: [u32; 3],
        entry_player_position: Option<[f32; 3]>,
    }
    let mut s: State = serde_json::from_slice(request)?;
    if s.submode > 4 || !s.private28.is_finite() || !s.work10.is_finite() {
        return Err(Error::invalid(
            "expected a mode-nine submode 0..4 and finite work fields",
        ));
    }
    let layout = DiscLayout::new(bytes, "boss_hotshotD.rel")?;
    let rel = layout.rel()?;
    let target = if let Some(pos) = s.entry_player_position {
        if s.submode != 0 || !pos.iter().all(|v| v.is_finite()) {
            return Err(Error::invalid(
                "target construction requires a finite player position on mode-nine entry",
            ));
        }
        let address = |at, section| -> Result<u32> {
            let hi = rel.reference(1, at).ok_or_else(invalid)?;
            if (hi.module, hi.section, hi.kind) != (rel.module_id(), section, 6)
                || rel.reference(1, at + 4) != Some(Reference { kind: 4, ..hi })
            {
                return Err(invalid());
            }
            Ok(hi.addend)
        };
        let unit = hot_shot_scalar(&rel, 0x4d132)?;
        Some(serde_json::json!({
            "sourcePlayerSlot":0,"position":pos,"savedPosition":pos,"scale":vec![unit;3],
            "work3c":hot_shot_scalar(&rel,0x4b86)?,"work44":hot_shot_scalar(&rel,0x4d14a)?,
            "updateSection1":address(0x4d0aa,1)?,"displaySection1":address(0x4d186,1)?,
            "destroySection1":address(0x4d192,1)?,"collisionSection5":address(0x4d15e,5)?,
            "workSize":84,"bossWork54":pos
        }))
    } else {
        None
    };
    let mut mode = 9;
    let mut next_variant = None;
    // Entry deliberately falls through into the first countdown on this same update.
    if s.submode == 0 {
        s.remaining = if s.private28 > hot_shot_scalar(&rel, 0x4b42)? {
            1
        } else {
            3
        };
        s.timer = 300;
        s.collision_flags &= !0x10;
        s.submode = 1;
    }
    match s.submode {
        1 => {
            s.timer = s.timer.wrapping_sub(1);
            if s.timer == 0 {
                s.submode = 2;
            }
        }
        2 => s.submode = 3,
        3 => {
            s.remaining = s.remaining.wrapping_sub(1);
            if s.remaining == 0 {
                s.timer = 60;
                s.submode = 4;
            } else {
                s.timer = 300;
                s.submode = 1;
            }
        }
        4 => {
            s.timer = s.timer.wrapping_sub(1);
            if s.timer == 0 {
                next_variant = Some(if s.private28 > hot_shot_scalar(&rel, 0x4d26)? {
                    2
                } else {
                    1
                });
                mode = 3;
                s.submode = 0;
                s.flags &= !0x200;
                s.collision_flags |= 0x10;
            }
        }
        _ => unreachable!(),
    }
    Ok(
        serde_json::json!({"mode":mode,"submode":s.submode,"timer":s.timer,"remaining":s.remaining,
        "targetOnSuccessfulAllocation":target,"privateByte2Write":next_variant,"flags":s.flags,"collisionFlags":s.collision_flags,
        "work10":s.work10+hot_shot_scalar(&rel,0x4dc6)?,
        "angles":[s.angles[0].wrapping_add(0x210),s.angles[1].wrapping_add(0x250),s.angles[2].wrapping_add(0x300)],
        "scope":"Mode-nine timing and selected writes. Target tracking, helper lifetimes and attack effects are not simulated."}),
    )
}

/// Tracking helper at 1:4cef4, called on the enabled, unlocked target update path.
pub fn hot_shot_target_follow(bytes: &[u8], request: &[u8]) -> Result<serde_json::Value> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct State {
        position: [f32; 3],
        saved_position: [f32; 3],
        player_position: [f32; 3],
        engine_length: f32,
        normalized: Option<[f32; 3]>,
    }
    let s: State = serde_json::from_slice(request)?;
    let layout = DiscLayout::new(bytes, "boss_hotshotD.rel")?;
    let rel = layout.rel()?;
    let speed = hot_shot_scalar(&rel, 0x4b86)?;
    let limited = s.engine_length > speed;
    if !s.engine_length.is_finite()
        || s.engine_length < 0.
        || limited != s.normalized.is_some()
        || !s
            .position
            .iter()
            .chain(&s.saved_position)
            .chain(&s.player_position)
            .chain(s.normalized.iter().flatten())
            .all(|v| v.is_finite())
    {
        return Err(Error::invalid(
            "expected finite tracking state and a normalization result only on the limited branch",
        ));
    }
    let mut delta: [f32; 3] = std::array::from_fn(|i| s.saved_position[i] - s.position[i]);
    if let Some(direction) = s.normalized {
        let [x, y, z] = delta;
        let squared = z * z + (x * x + y * y);
        if !squared.is_finite() {
            return Err(Error::invalid("target distance overflow"));
        }
        let step = if squared.sqrt() > hot_shot_scalar(&rel, 0x4d272)? {
            speed * hot_shot_scalar(&rel, 0x4d286)?
        } else {
            speed
        };
        delta = direction.map(|v| v * step);
    }
    let position: [f32; 3] = std::array::from_fn(|i| s.position[i] + delta[i]);
    if !position.iter().all(|v| v.is_finite()) {
        return Err(Error::invalid("target position overflow"));
    }
    Ok(
        serde_json::json!({"position":position,"savedPosition":s.player_position,
        "limited":limited,"scope":"Unlocked target movement; imported length and normalization results are supplied."}),
    )
}

/// Target update through the phase wrap at section 1:4d124.
pub fn hot_shot_target_clock(bytes: &[u8], request: &[u8]) -> Result<serde_json::Value> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct State {
        global37: i8,
        lock_word: i32,
        phase: f32,
        rate: f32,
        counter: u16,
    }
    let mut s: State = serde_json::from_slice(request)?;
    if !s.phase.is_finite() || !s.rate.is_finite() {
        return Err(Error::invalid("expected finite target phase and rate"));
    }
    let layout = DiscLayout::new(bytes, "boss_hotshotD.rel")?;
    let rel = layout.rel()?;
    let c = |at| hot_shot_scalar(&rel, at);
    let follows = s.global37 == 0 && s.lock_word == 0;
    if s.global37 == 0 {
        if follows {
            s.phase += s.rate;
            if s.counter == 60 {
                s.counter = 0;
                s.rate += c(0x4d3b2)?;
            } else {
                s.counter = s.counter.wrapping_add(1);
            }
        } else {
            s.phase += c(0x4d3ce)?;
            s.counter = 0;
            s.rate = c(0x4d3ea)?;
        }
        let wrap = c(0x4d3fe)?;
        if s.phase >= wrap {
            s.phase -= wrap;
        }
    }
    if !s.phase.is_finite() || !s.rate.is_finite() {
        return Err(Error::invalid("target clock overflow"));
    }
    Ok(
        serde_json::json!({"phase":s.phase,"rate":s.rate,"counter":s.counter,"callsFollow":follows,
        "skipsUpdate":s.global37!=0,"scope":"Target update gate and phase counter only."}),
    )
}

/// Orientation arithmetic of an enabled target update (1:4d128..4d340).
pub fn hot_shot_target_rotation(bytes: &[u8], request: &[u8]) -> Result<serde_json::Value> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct State {
        fixed: [i32; 3],
        varied: [i32; 3],
        engine_samples: [i32; 3],
    }
    let mut s: State = serde_json::from_slice(request)?;
    let layout = DiscLayout::new(bytes, "boss_hotshotD.rel")?;
    let rel = layout.rel()?;
    let hi = rel.reference(1, 0x4d422).ok_or_else(invalid)?;
    if (hi.module, hi.section, hi.kind) != (rel.module_id(), 5, 6)
        || rel.reference(1, 0x4d426) != Some(Reference { kind: 4, ..hi })
    {
        return Err(invalid());
    }
    let start = hi.addend as usize;
    let table = rel.data(5)?.get(start..start + 24).ok_or_else(invalid)?;
    let step = |i: usize| i32::from_be_bytes(table[i * 4..i * 4 + 4].try_into().unwrap());
    let scale = hot_shot_scalar(&rel, 0x4d482)?;
    let base = hot_shot_scalar(&rel, 0x4d492)?;
    for i in 0..3 {
        s.fixed[i] = s.fixed[i].wrapping_add(step(i));
        let weight = base + base * (scale * s.engine_samples[i] as f32);
        let next = s.varied[i] as f32 + step(i + 3) as f32 * weight;
        // Keep out-of-range fctiwz behavior unresolved for the installed hardware.
        if !(-2147483648.0..2147483648.0).contains(&next) {
            return Err(Error::invalid(
                "target rotation exceeds verified fctiwz range",
            ));
        }
        s.varied[i] = next as i32;
    }
    Ok(serde_json::json!({"fixed":s.fixed,"varied":s.varied}))
}

/// Resolve the two animated draws made by the target's display callback (1:4d42c).
pub fn hot_shot_target_display(bytes: &[u8], request: &[u8]) -> Result<serde_json::Value> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct State {
        flags: u16,
        position: [f32; 3],
        varied: [i32; 3],
        scale: [f32; 3],
        phase: f32,
    }
    let s: State = serde_json::from_slice(request)?;
    let layout = DiscLayout::new(bytes, "boss_hotshotD.rel")?;
    if s.flags & 0x200 != 0 {
        return Ok(serde_json::json!({"hidden":true,"draws":[]}));
    }
    if !s.position.iter().chain(&s.scale).all(|v| v.is_finite()) || !s.phase.is_finite() {
        return Err(Error::invalid("expected finite target display state"));
    }
    let rel = layout.rel()?;
    let ptr = |at| -> Result<u32> {
        let hi = rel.reference(1, at).ok_or_else(invalid)?;
        if (hi.module, hi.section, hi.kind) != (rel.module_id(), 5, 6)
            || rel.reference(1, at + 4) != Some(Reference { kind: 4, ..hi })
        {
            return Err(invalid());
        }
        Ok(hi.addend)
    };
    let base = ptr(0x4d7ce)? as usize;
    let data = rel.data(5)?;
    let fixed: Vec<i32> = data
        .get(base..base + 12)
        .ok_or_else(invalid)?
        .chunks_exact(4)
        .map(|w| i32::from_be_bytes(w.try_into().unwrap()))
        .collect();
    let [x, y, z] = s.position;
    let floor = hot_shot_scalar(&rel, 0x4d79e)?;
    let offset = hot_shot_scalar(&rel, 0x4d806)?;
    let ground = hot_shot_scalar(&rel, 0x4d842)?;
    Ok(
        serde_json::json!({"hidden":false,"textureNames":rel.texture_names(ptr(0x4d786)?)?,"textureListSection5":ptr(0x4d786)?,"phase":s.phase,
        "draws":[{"objectSection5":ptr(0x4d81a)?,"motionSection5":ptr(0x4d822)?,
        "translation":[x,y.max(floor),z],"rotations":[fixed,s.varied],"rotationArgument":1,
        "scale":s.scale,"localTranslation":[0.,offset,0.]},
        {"objectSection5":ptr(0x4d856)?,"motionSection5":ptr(0x4d85e)?,"translation":[x,ground,z]}]}),
    )
}

#[cfg(test)]
mod disc_layout_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    #[ignore = "requires SA2_GAME_PATH (Steam) and SA2_GC_REL_DIR (GameCube disc RELs)"]
    fn steam_boss_rels_match_their_disc_copies() {
        let steam =
            PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap()).join("resource/gd_PC");
        let disc = PathBuf::from(std::env::var_os("SA2_GC_REL_DIR").unwrap());
        for (file, inserts) in BOSS_INSERTS {
            let (a, b) = (
                std::fs::read(steam.join(file)).unwrap(),
                std::fs::read(disc.join(file)).unwrap(),
            );
            let (a, b) = (
                DiscLayout::new(&a, file).unwrap(),
                DiscLayout::new(&b, file).unwrap(),
            );
            let (a, b) = (a.rel().unwrap(), b.rel().unwrap());
            let inside = |s: u8, o: u32| {
                inserts
                    .iter()
                    .any(|&(t, at, n)| t == s && (at..at + n).contains(&o))
            };
            assert_eq!(a.module_id(), b.module_id());
            for s in 0..a.sections().len() as u8 {
                let (x, y) = (a.sections()[s as usize], b.sections()[s as usize]);
                assert_eq!(x.size, y.size, "{file} section {s}");
                if x.file_offset == 0 {
                    continue;
                }
                let (x, y) = (a.data(s).unwrap(), b.data(s).unwrap());
                for o in 0..x.len() as u32 {
                    let relocated = (o.saturating_sub(3)..=o).any(|p| b.reference(s, p).is_some());
                    if !inside(s, o) && !relocated {
                        assert_eq!(x[o as usize], y[o as usize], "{file} {s}:{o:#x}");
                    }
                }
            }
            let outside = |r: &CityRel<'_>| {
                r.refs
                    .iter()
                    .filter(|((s, o), _)| !inside(*s, *o))
                    .map(|(k, v)| (*k, *v))
                    .collect::<Vec<_>>()
            };
            assert_eq!(outside(&a), outside(&b), "{file} relocations");
        }
    }
}
