//! Sonic vs Shadow 2 (story level 42) platform terrain. The stage has no
//! landtable: the DODAIGEN object spawns copies of one Basic platform (DODAI)
//! along +X ahead of the players. This module reads that template and the
//! generator constants from the fingerprinted GC module and replays the
//! generator's placement rule. Platform falling, meteors and player response
//! are not simulated; no surface flags are inferred beyond the native
//! dynamic-collision registration word.
use crate::{
    city_land,
    city_rel::{CityRel, Reference},
    collision, manifest,
    model::{self, DecodeBudget, Node},
    object_table,
    stage::{self, BasicAttachment},
    Error, Result,
};
use serde::{Deserialize, Serialize};
pub const PLATFORM_RVA: u32 = 0x1251fa0;
/// Installed Steam `resource/gd_PC/stg42D.rel`.
pub const REL_SHA256: &str = "51d7b743c95cc8533e402533d73d43abab04c4de2e08770d615e143e1925385d";
pub const REL_LEN: usize = 345_422;
const MODULE: u32 = 41;
/// DODAI's template NJS_OBJECT in data section 5.
const TEMPLATE: u32 = 0x13c8;
/// Engine object allocator imported by both DODAIGEN spawn sites.
const SPAWN_IMPORT: u32 = 0x8007_1ba4;
/// Upper bound on platforms one generator update may create.
const MAX_SPAWNS_PER_FRAME: usize = 4096;
const MAX_FRAMES: usize = 1 << 20;

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

fn module(bytes: &[u8]) -> Result<CityRel<'_>> {
    if bytes.len() != REL_LEN || manifest::sha256(bytes) != REL_SHA256 {
        return Err(Error::invalid("unsupported stage 42 REL revision"));
    }
    CityRel::read_identified(bytes, MODULE)
}
fn invalid() -> Error {
    Error::invalid("unexpected stage 42 platform code")
}
/// The data-section target of the LO half of a `lis/addi` pair at `pc`.
fn target(rel: &CityRel<'_>, pc: u32, section: u8) -> Result<u32> {
    match rel.reference(1, pc) {
        Some(Reference {
            module: MODULE,
            kind: 4,
            section: s,
            addend,
        }) if s == section => Ok(addend),
        _ => Err(invalid()),
    }
}
fn float(rel: &CityRel<'_>, pc: u32) -> Result<f32> {
    let at = target(rel, pc, 4)? as usize;
    let b = rel.data(4)?.get(at..at + 4).ok_or_else(invalid)?;
    Ok(f32::from_be_bytes(b.try_into().unwrap()))
}
fn instruction(rel: &CityRel<'_>, pc: u32, expected: u32) -> Result<()> {
    let at = pc as usize;
    let b = rel.data(1)?.get(at..at + 4).ok_or_else(invalid)?;
    if u32::from_be_bytes(b.try_into().unwrap()) != expected {
        return Err(invalid());
    }
    Ok(())
}

/// Read DODAI's template object from the installed GC module (section 5 +0x13C8).
pub fn read_rel(rel_bytes: &[u8]) -> Result<Platform> {
    let rel = module(rel_bytes)?;
    // DODAI's constructor copies this object and its Basic model before use.
    if target(&rel, 0x1b96, 5)? != TEMPLATE {
        return Err(invalid());
    }
    city_land::rel_basic_object(&rel, 5, TEMPLATE as usize)
}

/// Constants of DODAIGEN (code +0x2278) and the DODAI fields it writes,
/// each read through the instruction that consumes it.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Generator {
    pub start_x: f32,
    pub step_x: f32,
    /// Spawning continues while `cursor - player.x` is below this distance.
    pub lookahead: f32,
    pub timer_scale: f32,
    pub timer_floor: f32,
    /// Single platform spawned instead when game byte +0x42 is set.
    pub result_x: f32,
    pub result_timer: u16,
    /// After `deadline_frames` unpaused updates, new platforms start activated.
    pub deadline_frames: u16,
    pub late_timer: u16,
    /// DODAI begins to fall on the first update with a signed timer above this.
    pub fall_after: i16,
    /// Timer assigned when a player first stands on a platform.
    pub stand_timer: u16,
    pub collision_radius_pad: f32,
    pub collision_flags: u32,
    pub collision_radius: f32,
}
pub fn generator(rel_bytes: &[u8]) -> Result<Generator> {
    let rel = module(rel_bytes)?;
    for pc in [0x22f2, 0x235e] {
        if target(&rel, pc, 1)? != 0x1d68 {
            return Err(invalid());
        }
    }
    for pc in [0x22fc, 0x23bc] {
        match rel.reference(1, pc) {
            Some(Reference {
                module: 0,
                kind: 10,
                addend: SPAWN_IMPORT,
                ..
            }) => {}
            _ => return Err(invalid()),
        }
    }
    for (pc, word) in [
        (0x231c, 0x3c80_0001), // lis r4,1
        (0x2320, 0x3804_8000), // addi r0,r4,-0x8000
        (0x2420, 0x3800_0384), // li r0,900
        (0x2484, 0x2800_8ca0), // cmplwi r0,36000
        (0x2490, 0x3803_8ca0), // addi r0,r3,-0x7360 (36000)
        (0x1e28, 0x2c00_0384), // cmpwi r0,900
        (0x2218, 0x3800_02d0), // li r0,720
        (0x1ca8, 0x3c80_0800), // lis r4,0x800
        (0x1cac, 0x3804_0001), // addi r0,r4,1
        (0x1cf4, 0x6400_2000),
        (0x1d10, 0x6400_4000),
        (0x1d18, 0x6400_6000),
    ] {
        instruction(&rel, pc, word)?;
    }
    let pad = float(&rel, 0x1cba)?;
    let template = read_rel(rel_bytes)?;
    // DODAI enlarges its private model copy's radius before registering it.
    let radius = template.attachment.bounds_radius + pad;
    let class = if radius > float(&rel, 0x1cd2)? {
        0
    } else if radius > float(&rel, 0x1ce6)? {
        0x2000_0000
    } else if radius > float(&rel, 0x1d02)? {
        0x4000_0000
    } else {
        0x6000_0000
    };
    Ok(Generator {
        start_x: float(&rel, 0x22b6)?,
        result_x: float(&rel, 0x230e)?,
        lookahead: float(&rel, 0x2376)?,
        timer_scale: float(&rel, 0x234e)?,
        timer_floor: float(&rel, 0x2356)?,
        step_x: float(&rel, 0x236e)?,
        result_timer: 0x8000,
        deadline_frames: 0x8ca0,
        late_timer: 0x384,
        fall_after: 0x384,
        stand_timer: 0x2d0,
        collision_radius_pad: pad,
        collision_flags: 0x0800_0001 | class,
        collision_radius: radius,
    })
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeneratorState {
    /// DODAIGEN work byte +0.
    pub mode: i8,
    /// Work +0x14.
    pub cursor_x: f32,
    /// Work +4 (bit 0x1000 marks the deadline).
    pub flags: u16,
    /// Work +6.
    pub frames: u16,
}
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FrameInput {
    /// X of each active player-table slot (absent slot: `null`), in table order.
    pub players: [Option<f32>; 2],
    /// Game byte +0x42 (nonzero selects the single result platform).
    #[serde(default)]
    pub result_mode: bool,
    /// Game byte +0x37 (nonzero stops the deadline counter).
    #[serde(default)]
    pub paused: bool,
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Spawn {
    /// DODAI work +0x14. Y, Z and angles are not written by the generator.
    pub x: f32,
    /// DODAI work +6.
    pub timer: u16,
    /// DODAI work +3.
    pub activated: bool,
}
impl Generator {
    /// One DODAIGEN update. Allocation is assumed to succeed.
    pub fn step(&self, s: &mut GeneratorState, input: &FrameInput) -> Result<Vec<Spawn>> {
        let mut out = Vec::new();
        match s.mode {
            0 => {
                s.cursor_x = self.start_x;
                s.mode = 1;
            }
            1 if input.result_mode => {
                out.push(Spawn {
                    x: self.result_x,
                    timer: self.result_timer,
                    activated: false,
                });
                s.mode = 2;
            }
            1 => {
                for px in input.players.iter().flatten() {
                    while s.cursor_x - px < self.lookahead {
                        if out.len() == MAX_SPAWNS_PER_FRAME {
                            return Err(Error::invalid("platform spawn limit"));
                        }
                        let mut t = self.timer_scale * (px - s.cursor_x);
                        // fcmpo/bge: keep t only when t >= floor (NaN selects the floor).
                        if t.partial_cmp(&self.timer_floor).is_none_or(|o| o.is_lt()) {
                            t = self.timer_floor;
                        }
                        let late = s.flags & 0x1000 != 0;
                        out.push(Spawn {
                            x: s.cursor_x,
                            timer: if late {
                                self.late_timer
                            } else {
                                t as i32 as u16
                            },
                            activated: late,
                        });
                        s.cursor_x += self.step_x;
                    }
                }
            }
            _ => {}
        }
        if !input.paused {
            s.frames = s.frames.wrapping_add(1);
            if s.frames > self.deadline_frames {
                s.frames = self.deadline_frames;
                s.flags |= 0x1000;
            }
        }
        Ok(out)
    }
}
impl Spawn {
    /// True unless DODAI starts falling on its first state-1 update.
    pub fn persists(&self, g: &Generator) -> bool {
        !self.activated && self.timer as i16 <= g.fall_after
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameReplay {
    pub state: GeneratorState,
    pub spawns: Vec<Spawn>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    frames: Vec<FrameInput>,
}
/// Replay DODAIGEN for each requested frame from a freshly loaded work area.
pub fn replay(rel_bytes: &[u8], request: &[u8]) -> Result<serde_json::Value> {
    let g = generator(rel_bytes)?;
    let req: Request = serde_json::from_slice(request)
        .map_err(|e| Error::invalid(format!("duel generator request: {e}")))?;
    if req.frames.len() > MAX_FRAMES {
        return Err(Error::invalid("duel generator frame limit"));
    }
    let mut state = GeneratorState::default();
    let mut frames = Vec::with_capacity(req.frames.len());
    for f in &req.frames {
        let spawns = g.step(&mut state, f)?;
        frames.push(FrameReplay { state, spawns });
    }
    Ok(serde_json::json!({ "generator": g, "frames": frames }))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacedPlatform {
    pub x: f32,
    pub timer: u16,
    pub activated: bool,
    pub persists: bool,
    /// First vertex of this platform in `vertices`, when it persists.
    pub first_vertex: Option<u32>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Terrain {
    pub source_sha256: &'static str,
    pub coordinate_space: &'static str,
    pub generator: Generator,
    pub players_x: [Option<f32>; 2],
    pub template: Platform,
    pub platforms: Vec<PlacedPlatform>,
    /// World-space collision of the persisting platforms only.
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
    /// Degenerate strip triples skipped once in the shared template.
    pub template_degenerate_triangles: usize,
    pub unknowns: Vec<&'static str>,
}
/// The platforms present after DODAIGEN's first two updates with the
/// players at the given X positions (normally the authored start rows).
pub fn start_terrain(rel_bytes: &[u8], players_x: [Option<f32>; 2]) -> Result<Terrain> {
    let g = generator(rel_bytes)?;
    let template = read_rel(rel_bytes)?;
    let n = &template.node;
    if n.scale != [1.0; 3] {
        return Err(Error::invalid("unexpected platform template scale"));
    }
    let mut local = Vec::new();
    let mut degenerate = 0;
    for set in &template.attachment.meshsets {
        for poly in &set.polygons {
            for i in 0..collision::triangle_count(set, poly)? {
                let t = collision::triangle_indices(set, poly, i);
                if t.iter()
                    .any(|&j| j as usize >= template.attachment.vertices.len())
                {
                    return Err(Error::invalid("platform index outside vertices"));
                }
                if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
                    degenerate += 1;
                } else {
                    local.push(t);
                }
            }
        }
    }
    let mut state = GeneratorState::default();
    let input = FrameInput {
        players: players_x,
        ..Default::default()
    };
    let mut spawns = g.step(&mut state, &input)?;
    spawns.extend(g.step(&mut state, &input)?);
    let (mut vertices, mut triangles, mut platforms) = (Vec::new(), Vec::new(), Vec::new());
    for s in spawns {
        let persists = s.persists(&g);
        let first = vertices.len() as u32;
        if persists {
            vertices.extend(
                template
                    .attachment
                    .vertices
                    .iter()
                    .map(|v| [v[0] + s.x, v[1], v[2]]),
            );
            triangles.extend(local.iter().map(|t| t.map(|j| j + first)));
        }
        platforms.push(PlacedPlatform {
            x: s.x,
            timer: s.timer,
            activated: s.activated,
            persists,
            first_vertex: persists.then_some(first),
        });
    }
    Ok(Terrain {
        source_sha256: REL_SHA256,
        coordinate_space: "native SA2 world units; Y up; platform origin at (x, 0, 0)",
        generator: g,
        players_x,
        template,
        platforms,
        vertices,
        triangles,
        template_degenerate_triangles: degenerate,
        unknowns: vec![
            "Spawned DODAI work Y, Z and angles are taken as zero: the generator writes only X, the timer and the activation byte, and the engine allocator was not executed.",
            "Later platforms appear as players advance; use Generator::step per frame. Falling, timers after landing, meteors and player response are not simulated.",
        ],
    })
}
