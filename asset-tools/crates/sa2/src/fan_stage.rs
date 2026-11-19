//! Offline mount of an authored Hero, Dark or Last campaign level entry.
use crate::{
    city_replay, collision,
    manifest::sha256,
    object_table::{self, ObjectDefinition, ObjectList},
    set,
    stage::Stage,
    stage_catalog::{sections, BoundPlacement},
    stage_positions::{self, PositionKind, Variant},
    story, Error, Result,
};
use serde::Serialize;
use std::collections::BTreeMap;

const STORIES: [&str; 3] = [
    "Hero Story Sequence",
    "Dark Story Sequence",
    "Last Story Sequence",
];
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelDispatch {
    pub sequence: &'static str,
    pub action_index: usize,
    pub source_rva: u32,
    pub character: i8,
    pub level: u16,
}
const DEMOS: [&str; 7] = [
    "Sonic SA2 The Trial Demo Sequence",
    "Sonic Demo Sequence",
    "Mech Tails Demo Sequence",
    "Knuckles Demo Sequence",
    "Shadow Demo Sequence",
    "Mech Eggman Demo Sequence",
    "Rouge Demo Sequence",
];
const BOSS_RUSHES: [&str; 3] = [
    "Hero Boss Rush Story Sequence",
    "Dark Boss Rush Story Sequence",
    "All Boss Rush Story Sequence",
];
struct LevelConfig {
    section: String,
    set_prefix: String,
    geometry: Option<String>,
    object_section: Option<String>,
}
/// Immutable index of normal story routes; resource bytes are requested lazily by mount.
pub struct Campaign {
    levels: Vec<LevelDispatch>,
    boss_rushes: Vec<LevelDispatch>,
    demos: Vec<LevelDispatch>,
    configs: BTreeMap<u16, LevelConfig>,
    tables: BTreeMap<String, ObjectList>,
    positions: Vec<stage_positions::PositionRow>,
    kart_routes: BTreeMap<i8, KartRoute>,
    physics: Vec<crate::player_physics::PlayerPhysics>,
    ranks: Vec<crate::mission_ranks::RankRow>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KartRoute {
    pub name: &'static str,
    pub descriptor_rva: u32,
    pub physics_rva: u32,
    pub physics_sha256: String,
    pub physics: KartPhysics,
    pub rank_times_rva: u32,
    pub rank_times_sha256: String,
    pub rank_times: Vec<KartRankRow>,
    pub course_parts: Vec<u8>,
    pub set_filename: &'static str,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KartPhysics {
    pub acceleration: f32,
    pub brake_power: f32,
    pub deceleration: f32,
    pub speed_cap: f32,
    pub weight: f32,
    pub unknown1: f32,
    pub drift_handling: f32,
    pub drift_speed_threshold: f32,
    pub unknown2: f32,
    pub top_speed: f32,
}
#[derive(Debug, Serialize)]
pub struct KartRankRow {
    pub level: u8,
    /// Each pair stores native minutes and seconds, in that order.
    pub d: [u8; 2],
    pub c: [u8; 2],
    pub b: [u8; 2],
    pub a: [u8; 2],
}
#[derive(Debug, Serialize)]
pub struct KartProfile {
    pub slot: usize,
    pub name: &'static str,
    pub source_rva: u32,
    pub sha256: String,
    pub physics: KartPhysics,
}

// Names correspond to the split filenames in SA Tools PC_SA2/CART.ini.
const KART_PROFILES: [&str; 15] = [
    "TailsStory",
    "RougeStory",
    "SonicKart",
    "KnucklesKart",
    "TailsKart",
    "EggmanKart",
    "ShadowKart",
    "RougeKart",
    "Placeholder",
    "SonicAltKart",
    "KnucklesAltKart",
    "ChaoKart",
    "EggmanAltKart",
    "ShadowAltKart",
    "EggroboKart",
];
fn read_kart_profile(
    r: &crate::model::Reader<'_>,
    pe: &crate::pe::PeImage,
    slot: usize,
) -> Result<KartProfile> {
    let name = *KART_PROFILES
        .get(slot)
        .ok_or_else(|| Error::invalid("unknown kart profile slot"))?;
    let physics_rva = 0xbf9ef0 + 40 * slot as u32;
    let pa = pe.rva_to_offset(physics_rva, 40)?;
    let f = |i: usize| r.f32(pa + 4 * i);
    let physics = KartPhysics {
        acceleration: f(0)?,
        brake_power: f(1)?,
        deceleration: f(2)?,
        speed_cap: f(3)?,
        weight: f(4)?,
        unknown1: f(5)?,
        drift_handling: f(6)?,
        drift_speed_threshold: f(7)?,
        unknown2: f(8)?,
        top_speed: f(9)?,
    };
    let physics_sha256 = crate::manifest::sha256(r.span(pa, 40)?);
    Ok(KartProfile {
        slot,
        name,
        source_rva: physics_rva,
        sha256: physics_sha256,
        physics,
    })
}
pub fn kart_profiles(exe: &[u8]) -> Result<Vec<KartProfile>> {
    let pe = crate::object_table::verify_image(exe)?;
    let r = crate::model::Reader { data: exe };
    (0..KART_PROFILES.len())
        .map(|slot| read_kart_profile(&r, &pe, slot))
        .collect()
}

/// The two kart stage dispatches use separate course descriptors rather than a static landtable.
fn kart_routes(exe: &[u8]) -> Result<BTreeMap<i8, KartRoute>> {
    let pe = crate::pe::read_pe(exe)?;
    let r = crate::model::Reader { data: exe };
    let mut routes = BTreeMap::new();
    for (ch, name, descriptor_rva, expected, set_filename, profile_slot, rank_times_rva) in [
        (
            6,
            "Route 101",
            0xaf82bc,
            321,
            "setCartTails.bin",
            0,
            0x133b378,
        ),
        (
            5,
            "Route 280",
            0xaf82c4,
            369,
            "setCartRouge.bin",
            1,
            0x133b3a5,
        ),
    ] {
        let ids = crate::kart_course::read_piece_ids(&r, &pe, descriptor_rva, expected)?;
        let profile = read_kart_profile(&r, &pe, profile_slot)?;
        let ranks = r.span(pe.rva_to_offset(rank_times_rva, 45)?, 45)?;
        let mut rank_times = Vec::with_capacity(5);
        for &[level, dm, ds, cm, cs, bm, bs, am, asec] in ranks.as_chunks::<9>().0 {
            if level != 70 || [ds, cs, bs, asec].iter().any(|s| *s >= 60) {
                return Err(Error::invalid("unexpected kart rank row"));
            }
            rank_times.push(KartRankRow {
                level,
                d: [dm, ds],
                c: [cm, cs],
                b: [bm, bs],
                a: [am, asec],
            });
        }
        routes.insert(
            ch,
            KartRoute {
                name,
                descriptor_rva,
                physics_rva: profile.source_rva,
                physics_sha256: profile.sha256,
                physics: profile.physics,
                rank_times_rva,
                rank_times_sha256: crate::manifest::sha256(ranks),
                rank_times,
                course_parts: ids.to_vec(),
                set_filename,
            },
        );
    }
    Ok(routes)
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryPose {
    pub character: &'static str,
    pub row_va: u32,
    pub position: [f32; 3],
    pub yaw: u16,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layer {
    pub filename: &'static str,
    pub sha256: &'static str,
    pub placements: Vec<BoundPlacement>,
    pub unresolved_ids: BTreeMap<u16, usize>,
    pub partial_callback_counts: BTreeMap<&'static str, usize>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Terrain {
    pub geometry_path: String,
    pub filename: &'static str,
    pub symbol: &'static str,
    pub entries: usize,
    pub basic_attachments: usize,
    #[serde(skip)]
    pub stage: Stage,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MountedStage<'a> {
    pub dispatch: LevelDispatch,
    pub level_section: Option<&'a str>,
    pub entry_pose: Option<EntryPose>,
    /// Authored single-player end pose; does not trigger completion or respawn.
    pub end_pose: Option<EntryPose>,
    pub mission2_end_pose: Option<EntryPose>,
    pub mission3_end_pose: Option<EntryPose>,
    /// Static defaults for the dispatched character, including when riding a kart.
    pub character_defaults: &'a crate::player_physics::PlayerPhysics,
    pub kart_route: Option<&'a KartRoute>,
    /// Authored rows only; absence does not imply an automatic grade.
    pub mission_ranks: Vec<&'a crate::mission_ranks::RankRow>,
    pub object_table: Option<&'a ObjectList>,
    #[serde(skip)]
    checkpoint_items: Option<crate::checkpoint::StageItemTable>,
    pub layers: Vec<Layer>,
    pub terrain: Option<Terrain>,
    /// Resource/entry-specific gaps. These are separate from missing gameplay systems.
    pub gaps: Vec<String>,
    pub unavailable_systems: &'static [&'static str],
}
impl MountedStage<'_> {
    /// Authored SET records bound to the Steam Checkpoint_Main callback.
    /// The layer and SET index identify the placement, not its runtime respawn ID.
    pub fn checkpoint_placements(&self) -> impl Iterator<Item = (&Layer, &BoundPlacement)> {
        let objects = self.object_table;
        self.layers.iter().flat_map(move |layer| {
            layer.placements.iter().filter_map(move |placement| {
                let def = objects?.entries.get(placement.definition_index?)?;
                (def.function_va == 0x6d99a0 && def.name == "SAVEPOINT")
                    .then_some((layer, placement))
            })
        })
    }

    /// Raw object function addresses are identities, not callable host pointers.
    pub fn resolve(
        &self,
        layer: usize,
        index: usize,
    ) -> Option<(&BoundPlacement, Option<&ObjectDefinition>)> {
        let p = self.layers.get(layer)?.placements.get(index)?;
        Some((
            p,
            p.definition_index
                .and_then(|i| self.object_table?.entries.get(i)),
        ))
    }
    pub fn collision_world(&self) -> Result<Option<collision::CollisionWorld>> {
        self.terrain
            .as_ref()
            .map(|t| collision::bake_stage(&t.stage, 1.))
            .transpose()
    }
    /// Replay a verified shared callback or a stage-specific City Escape callback.
    pub fn replay_object(
        &self,
        layer: usize,
        index: usize,
        request: &[u8],
    ) -> Result<serde_json::Value> {
        let (placement, definition) = self
            .resolve(layer, index)
            .ok_or_else(|| Error::invalid("placement outside mounted layer"))?;
        city_replay::mounted_replay(
            self.checkpoint_items.as_ref(),
            self.dispatch.level,
            &placement.placement,
            definition.ok_or_else(|| Error::invalid("unresolved native object ID"))?,
            request,
        )
    }
}

impl Campaign {
    pub fn new(
        exe: &[u8],
        story_config: &str,
        level_config: &str,
        object_config: &str,
    ) -> Result<Self> {
        for (text, expected) in [
            (
                story_config,
                "786f9083923d5f2dbec73fdb0be284154ced16c433b335386bba0af89272a3a0",
            ),
            (
                level_config,
                "854a458f0a3b868194727988b7276830dd7a87bc383939d191945754032035c4",
            ),
            (
                object_config,
                "85fb9c71b82690f806ba59fe977eb390c73932c84594484e10f7f62a31cdbabd",
            ),
        ] {
            if sha256(text.as_bytes()) != expected {
                return Err(Error::invalid("unsupported campaign config revision"));
            }
        }
        let sequences = story::read(exe, story_config)?;
        let collect_levels = |names: &[&'static str]| -> Result<Vec<LevelDispatch>> {
            let mut rows = Vec::new();
            for &sequence in names {
                let mut cursor = story::Cursor::new(
                    sequences
                        .get(sequence)
                        .ok_or_else(|| Error::invalid("missing campaign sequence"))?,
                );
                let mut action_index = 0;
                while let Some(d) = cursor.dispatch() {
                    if let story::Action::Level { character, level } = d.action {
                        rows.push(LevelDispatch {
                            sequence,
                            action_index,
                            source_rva: d.source_rva,
                            character,
                            level: level as u16,
                        });
                    }
                    cursor.acknowledge(&d.token)?;
                    action_index += 1;
                }
            }
            Ok(rows)
        };
        let levels = collect_levels(&STORIES)?;
        let boss_rushes = collect_levels(&BOSS_RUSHES)?;
        let demos = collect_levels(&DEMOS)?;
        let mut names = BTreeMap::new();
        for (section, fields) in sections(object_config, &["filename", "type"])? {
            if fields.get("type").is_some_and(|v| v == "objlist") {
                let key = fields
                    .get("filename")
                    .ok_or_else(|| Error::invalid("missing object filename"))?
                    .replace('\\', "/")
                    .to_ascii_lowercase();
                if names.insert(key, section).is_some() {
                    return Err(Error::invalid("ambiguous object list name"));
                }
            }
        }
        let mut configs = BTreeMap::new();
        for (section, fields) in sections(
            level_config,
            &["levelid", "setname", "levelgeometry", "objectlist"],
        )? {
            let Some(id) = fields.get("levelid").and_then(|s| s.parse::<u16>().ok()) else {
                continue;
            };
            if !levels
                .iter()
                .chain(&boss_rushes)
                .chain(&demos)
                .any(|l| l.level == id)
            {
                continue;
            }
            let object_section = fields
                .get("objectlist")
                .and_then(|f| names.get(&f.replace('\\', "/").to_ascii_lowercase()))
                .cloned();
            let set_prefix = fields.get("setname").map_or_else(
                || format!("set{id:04}"),
                |s| format!("set{}", s.to_ascii_lowercase()),
            );
            let c = LevelConfig {
                section,
                set_prefix,
                object_section,
                geometry: fields.get("levelgeometry").cloned(),
            };
            if configs.insert(id, c).is_some() {
                return Err(Error::invalid("ambiguous campaign level section"));
            }
        }
        Ok(Self {
            levels,
            boss_rushes,
            demos,
            configs,
            tables: object_table::mapped_tables(exe, object_config)?,
            positions: stage_positions::read_rows(exe)?,
            kart_routes: kart_routes(exe)?,
            physics: crate::player_physics::read(exe)?,
            ranks: crate::mission_ranks::read(exe)?,
        })
    }
    pub fn rank_rows(&self, dispatch: &LevelDispatch) -> Vec<&crate::mission_ranks::RankRow> {
        let course = if dispatch.level == 70 {
            let Some(route) = self.kart_routes.get(&dispatch.character) else {
                return vec![];
            };
            Some(route.name)
        } else {
            None
        };
        self.ranks
            .iter()
            .filter(|r| r.level == dispatch.level && r.course == course)
            .collect()
    }
    pub fn demos(&self) -> &[LevelDispatch] {
        &self.demos
    }
    pub fn boss_rushes(&self) -> &[LevelDispatch] {
        &self.boss_rushes
    }
    pub fn dispatch(&self, sequence: &str, ordinal: usize) -> Option<&LevelDispatch> {
        self.levels
            .iter()
            .chain(&self.boss_rushes)
            .chain(&self.demos)
            .find(|d| d.sequence == sequence && d.action_index == ordinal)
    }
    pub fn levels(&self) -> &[LevelDispatch] {
        &self.levels
    }
    /// The resolver receives a pinned basename relative to resource/gd_PC. None means
    /// locally unavailable; a mismatched source is an error. No filesystem writes.
    pub fn mount(
        &self,
        sequence: &str,
        action_index: usize,
        mut load: impl FnMut(&str) -> Result<Option<Vec<u8>>>,
    ) -> Result<MountedStage<'_>> {
        let dispatch = self
            .dispatch(sequence, action_index)
            .ok_or_else(|| Error::invalid("not an authored single-player campaign level action"))?
            .clone();
        let character_defaults = usize::try_from(dispatch.character)
            .ok()
            .and_then(|slot| self.physics.get(slot))
            .ok_or_else(|| Error::invalid("unknown story character slot"))?;
        let ch = character_defaults
            .character
            .ok_or_else(|| Error::invalid("unnamed story character slot"))?;
        let lookup_pose = |kind, variant| -> Result<Option<EntryPose>> {
            let mut rows = self
                .positions
                .iter()
                .filter(|r| r.stage == dispatch.level && r.kind == kind && r.character == ch);
            let pose = rows.next().and_then(|r| {
                r.poses
                    .iter()
                    .find(|p| p.variant == variant)
                    .map(|p| EntryPose {
                        character: r.character,
                        row_va: r.row_va,
                        position: p.position,
                        yaw: p.yaw,
                    })
            });
            if rows.next().is_some() {
                return Err(Error::invalid("ambiguous campaign position row"));
            }
            Ok(pose)
        };
        let entry_pose = lookup_pose(PositionKind::Start, Variant::Single)?;
        let end_pose = lookup_pose(PositionKind::End, Variant::Single)?;
        let mission2_end_pose = lookup_pose(PositionKind::MissionEnd, Variant::Mission2)?;
        let mission3_end_pose = lookup_pose(PositionKind::MissionEnd, Variant::Mission3)?;
        let mut gaps = vec![];
        if entry_pose.is_none() {
            gaps.push(format!(
                "No single-player start row for {ch} and stage {}.",
                dispatch.level
            ));
        }
        let config = self.configs.get(&dispatch.level);
        let kart_route = if dispatch.level == 70 {
            self.kart_routes.get(&dispatch.character)
        } else {
            None
        };
        let object_table = if kart_route.is_some() {
            self.tables.get("Kart Object List")
        } else {
            config.and_then(|c| self.tables.get(c.object_section.as_ref()?))
        };
        let mut layers = vec![];
        let mut terrain = None;
        if let Some(c) = config {
            if object_table.is_none() {
                gaps.push("No native object list mapping.".into());
            }
            for suffix in ["s", "u"] {
                mount_layer(
                    &format!("{}_{suffix}.bin", c.set_prefix),
                    object_table,
                    dispatch.level,
                    &mut load,
                    &mut layers,
                    &mut gaps,
                )?;
            }
            if let Some(path) = &c.geometry {
                let candidates: Vec<_> = crate::stage_land::catalog()
                    .iter()
                    .filter(|p| p.supported && p.geometry_path.eq_ignore_ascii_case(path))
                    .collect();
                for source in &candidates {
                    let Some(bytes) = load(source.filename)? else {
                        continue;
                    };
                    if sha256(&bytes) != source.sha256 {
                        return Err(Error::invalid(format!(
                            "unexpected terrain source {}",
                            source.filename
                        )));
                    }
                    let stage = crate::stage_land::read_named(&bytes, source.symbol)?;
                    terrain = Some(Terrain {
                        geometry_path: path.clone(),
                        filename: source.filename,
                        symbol: source.symbol,
                        entries: stage.entries.len(),
                        basic_attachments: stage.basic_attachments.len(),
                        stage,
                    });
                    break;
                }
                if terrain.is_none() {
                    gaps.push(if candidates.is_empty() {
                        format!("No verified terrain reader for {path}.")
                    } else {
                        format!("No mounted source for {path}.")
                    });
                }
            } else {
                gaps.push("Level section declares no static landtable.".into());
            }
        } else if let Some(route) = kart_route {
            mount_layer(
                route.set_filename,
                object_table,
                dispatch.level,
                &mut load,
                &mut layers,
                &mut gaps,
            )?;
            gaps.push("Kart course part IDs are available; track assembly, driving and race conditions are not ported.".into());
        } else {
            gaps.push(format!(
                "No matching sa2lvl.ini section for campaign stage {}.",
                dispatch.level
            ));
        }
        let checkpoint_items = if layers.iter().flat_map(|layer| &layer.placements).any(|p| {
            p.definition_index
                .and_then(|i| object_table?.entries.get(i))
                .is_some_and(|d| d.function_va == 0x6d99a0 && d.name == "SAVEPOINT")
        }) {
            let table = load(&format!("stg{:02}D.rel", dispatch.level))?
                .as_deref()
                .map(|bytes| crate::checkpoint::StageItemTable::read_stage(dispatch.level, bytes))
                .transpose()?;
            if table.is_none() {
                gaps.push("Missing stage REL; checkpoint item dispatch unavailable.".into());
            }
            table
        } else {
            None
        };
        let mission_ranks = self.rank_rows(&dispatch);
        Ok(MountedStage {
            character_defaults,
            mission_ranks,
            checkpoint_items,
            dispatch,
            level_section: config.map(|c| c.section.as_str()),
            entry_pose,
            end_pose,
            mission2_end_pose,
            mission3_end_pose,
            kart_route,
            unavailable_systems: &[
                "retail task scheduling and dynamic contacts",
                "character movement and damage",
                "mission and boss execution",
                "stage-clear and save progression",
            ],
            object_table,
            layers,
            terrain,
            gaps,
        })
    }
}

fn mount_layer(
    requested: &str,
    object_table: Option<&ObjectList>,
    level: u16,
    load: &mut impl FnMut(&str) -> Result<Option<Vec<u8>>>,
    layers: &mut Vec<Layer>,
    gaps: &mut Vec<String>,
) -> Result<()> {
    let Some(&(filename, digest)) = NORMAL_SETS
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(requested))
    else {
        gaps.push(format!("No {requested} in the supported install."));
        return Ok(());
    };
    let Some(bytes) = load(filename)? else {
        gaps.push(format!("Missing local resource {filename}."));
        return Ok(());
    };
    if sha256(&bytes) != digest {
        return Err(Error::invalid(format!("unexpected bytes for {filename}")));
    }
    let mut placements = vec![];
    let mut unresolved_ids = BTreeMap::new();
    let mut partial_callback_counts = BTreeMap::new();
    for (placement_index, placement) in set::parse_set(&bytes)?.into_iter().enumerate() {
        let i = usize::from(placement.object_id);
        let def = object_table.and_then(|t| t.entries.get(i));
        if def.is_none() {
            *unresolved_ids.entry(placement.object_id).or_default() += 1;
        }
        if let Some(name) = def.and_then(|d| city_replay::mounted_callback(level, &placement, d)) {
            *partial_callback_counts.entry(name).or_default() += 1;
        }
        placements.push(BoundPlacement {
            placement_index,
            source_offset: 32 + 32 * placement_index,
            placement,
            definition_index: def.map(|_| i),
        });
    }
    layers.push(Layer {
        filename,
        sha256: digest,
        placements,
        unresolved_ids,
        partial_callback_counts,
    });
    Ok(())
}

// Installed normal layers only. Digests prevent a differently named or edited SET
// being mounted under another stage's object table.
const NORMAL_SETS: &[(&str, &str)] = &[
    (
        "setCartTails.bin",
        "16e8fed06995b564022c7621658fc8052f12103ebd1c4f162e74ab604d9aa4a5",
    ),
    (
        "setCartRouge.bin",
        "46938046069128d919c4aa39a79d442142f5ae0593bd0559605f0ea5bc52c39e",
    ),
    (
        "set0003_s.bin",
        "ea84c634082db5ad49cfd8a7c76a9ad722d1e4baa9198e8c9a0c50de556632c6",
    ),
    (
        "set0003_u.bin",
        "dfff716c34bdaa6173512a2f317c266ab3a3c61dbbeebee944d27c8eb6fc1862",
    ),
    (
        "set0004_s.bin",
        "831be5468dfef74acb29809fede9477245623d597a2faf764a96da2e507c452c",
    ),
    (
        "set0004_u.bin",
        "1665c5e56c5057fddcb76ec1a3a51193baf84eb6bcc95e017fb42fc9e0937c9f",
    ),
    (
        "set0005_s.bin",
        "b16f1196727f2f6e0e03407ed387fb30b8dc702c6985cc58dff1aef55b74732e",
    ),
    (
        "set0005_u.bin",
        "cfafc7885654f5041e2abd5cfc3a0b32cc8485ef49dbac103e9bf771cc37c87d",
    ),
    (
        "set0006_s.bin",
        "e13e460d828a728f21f395a68718749885a732106f76d7ee57cec35b3ae5511b",
    ),
    (
        "set0006_u.bin",
        "9299c338e59413e86eac03426589f093878c67bdc36217163535ffd3d1249b8e",
    ),
    (
        "set0007_s.bin",
        "dfec4205e83c2953aec911f9e35632ab1690af59bb82d20de6cf5fa41578295c",
    ),
    (
        "set0007_u.bin",
        "a37b5ecd44897a2605c473c519562579a82a940f9efcdfa9d60af37dc335d357",
    ),
    (
        "set0008_s.bin",
        "ead745f7158d7a4ff74fe07c3179a89db97d59808740b8f25c2d1e8533329312",
    ),
    (
        "set0008_u.bin",
        "6d3d095b1a674bf8e13190f7fa2551b7377db46940a15ff331c81a9d20c5e696",
    ),
    (
        "set0009_s.bin",
        "ae4b089f437ada56288701ddc13b0d72a6e5ae69765f519252636baa125a9402",
    ),
    (
        "set0009_u.bin",
        "efb6eefe22ca71ded8524b5bcbd7d9a9fd44ce5bf102d0575fa329908b1b60f5",
    ),
    (
        "set0010_s.bin",
        "7a26def0bd31f362eb652051e32a2a63b2e678bf894f32d5c7480c963d6ad2ec",
    ),
    (
        "set0010_u.bin",
        "24acd20feebdb5b4e382cfb2b172801816797989eba1d4884a94bafc5ed34fc8",
    ),
    (
        "set0011_s.bin",
        "e1f45e273225eaa4f4d171de2d6a3172e6cf7f3db74a40b48cab60098d268d99",
    ),
    (
        "set0011_u.bin",
        "0815fa2379512e70f7654fde5324e8d93bd3b7257802ff3624ee6dbd9ceab6fa",
    ),
    (
        "set0012_s.bin",
        "7d47cf43918624e52597c27e332a284a2c633d203c0c7f1291be3c0129247d5a",
    ),
    (
        "set0012_u.bin",
        "9b7d6568d84bb6e7958ffca8d0b35f8f2204a437473a42dfee258116755b4916",
    ),
    (
        "set0013_s.bin",
        "9e7ab581b5b85dd09a4eac1d5b34afd4785b49f17575cb78e4c3c814cf05ff14",
    ),
    (
        "set0013_u.bin",
        "96cc77030033fefb76330558a444c87c0e18693294977b60b7d071a64d3dfa32",
    ),
    (
        "set0014_s.bin",
        "987fb41a7d1ec901ac9bc80506e5a04807dd86dc1c1523609da27ad831393563",
    ),
    (
        "set0014_u.bin",
        "886d58b8ee82f8641381ea9215a35e5396f133a3be6894083f86331f0f63c48b",
    ),
    (
        "set0016_s.bin",
        "41806a66dcd8ce4ab957d708bd538e7141ee9e670412188ee4ced373d0f94f5d",
    ),
    (
        "set0016_u.bin",
        "e4111eb63698899b628b3de99b3b1b0f7d9faa01c900c395ae740c21bb0c68f7",
    ),
    (
        "set0017_s.bin",
        "dcc87cd3c04e2f0d64b51d46cb5b59f0b2874406e39d0302cea1145a3f02a175",
    ),
    (
        "set0017_u.bin",
        "7af68d07e1f33e6950166e1302da7c8df6b999d02a9da2407f0af464d3c4468b",
    ),
    (
        "set0018_s.bin",
        "c36b4a8a84a3ec67f311e17d9192668aee78c1c6ff521c5ebe1461669599375f",
    ),
    (
        "set0018_u.bin",
        "c5ee2fc857c020f817059cb5106d1defaae66b22709a6b62ff51502113c3cc04",
    ),
    (
        "set0019_s.bin",
        "8985ce817f19dffff8f0a92ec032e7cf826666a8fae3e8827b2b6528c90ce911",
    ),
    (
        "SET0019_U.BIN",
        "42d0bd351a9202bce0f861c1f0608f78cbe4d465dbf88b98e7b419d0fbd6383a",
    ),
    (
        "SET0020_S.BIN",
        "c9b7b12b57e5e382a8e8a47fe6dbfcb62733d267535b2b68041dd10585a92f44",
    ),
    (
        "SET0020_U.BIN",
        "4f13ea0bec5ac05656da7cf2c70737db8a97bc64e65c1e4ca14594a5264e6123",
    ),
    (
        "set0021_s.bin",
        "56076237fe7fddf3f026cf6101eec2b434d7ebc99c5c8bca5edb121692d78338",
    ),
    (
        "set0021_u.bin",
        "9ad051d101cdf8180ef9ac6bc06d27c6878f19b3a02e45113460d5991f5ce19e",
    ),
    (
        "set0022_s.bin",
        "919ae1b00494720f29d07d4cdcddbc7517303c6f9592d3bd96bdcdd0b06be0cd",
    ),
    (
        "set0022_u.bin",
        "6ff0f04933df6933bf1d8954486f7809a24a42b9671c9169b3db11d3ff3e01d0",
    ),
    (
        "set0023_s.bin",
        "7d04fa4188d476774503d5e857e6ad9bc52986e129256864f07428e50837cb9b",
    ),
    (
        "set0023_u.bin",
        "f13d6de9f6eb937860e3dac3dc8ba70af938282a1faa3ddfffff8feb6f848c8a",
    ),
    (
        "set0024_s.bin",
        "7038aee734e1e1792e9847a946068a9659d83b91122e71c6206c194e23963852",
    ),
    (
        "SET0024_U.BIN",
        "789324a4d4122ffe078cefe6ba4350cae95ea24a46d03a8d5a17df56f184aeb6",
    ),
    (
        "set0025_s.bin",
        "bdd054ad1d97acc38a3050ba4132f999f90e6c80fb883c1c8337ec2c87f00480",
    ),
    (
        "set0025_u.bin",
        "435816bbf3399c5b24aaaadf9319b64dc2fc1ce396d0599b71a6980a4b5bf17b",
    ),
    (
        "set0026_s.bin",
        "eceaa068e79ef024d02a2c034552f215e7493793df0f42d1cb7f06f54c2f35fb",
    ),
    (
        "set0026_u.bin",
        "6c5a1085a95a6a7a678433453e91447d6486c697edb5cb3dfc44d4d1fa6085ec",
    ),
    (
        "set0027_s.bin",
        "47e8c098bd5a67db27682d316e2183f935cac8b37cafbb9ca9f8942459c599a8",
    ),
    (
        "set0027_u.bin",
        "68ac09a50e758bf6576fba0dfa448cb77cb3aec764d441fc78be6377770ec09e",
    ),
    (
        "set0028_s.bin",
        "2e90dddfd9b4a26fd00ad127d25d7a3c152acda0942a3f9ed3a68ccb9a0f2b3c",
    ),
    (
        "set0028_u.bin",
        "968e2aa67ee9d06b7857dbc17ac1e6cd417d8c17a5d84b0aca3d02b615833e34",
    ),
    (
        "set0029_s.bin",
        "5d358fdae7f1ae28a41a279ced5c2e4314373b594b0d3ae2c481113adfb85631",
    ),
    (
        "set0029_u.bin",
        "8492e38287483f05052700ae62a7da2b634ba40a5aec182a4eb473ecc6cb2ef6",
    ),
    (
        "set0030_s.bin",
        "8c552cb1db4f126ff4ae983bf1680f9c0b51d8ca99598df272406cc486dc34f8",
    ),
    (
        "set0030_u.bin",
        "1bf969a6721974d879af87c2764046940d6f439ae69ee1df891279e2665da660",
    ),
    (
        "set0032_s.bin",
        "4c288e7fe90f043105b1bf550e53d3bc9eb5c06288887e8881dccd07dfd175e3",
    ),
    (
        "set0032_u.bin",
        "d054a0e8f92178df93cc516a8c8f37fdf7fd17968f3fa96cd38496de07ba9964",
    ),
    (
        "set0033_s.bin",
        "bf8ac27a16a8dcc9a98aca2307461a6d4601e6d5a28638c9f70417f9379b8dfb",
    ),
    (
        "set0033_u.bin",
        "d3aa42755310bf8b54bd411e76ebfae475de3d10161ec467c2897adc382c65f1",
    ),
    (
        "set0036_s.bin",
        "c64bfb2ab2d458b67d0a2336d0fa35a333b78ef52d818cc256218e408c12a1fa",
    ),
    (
        "set0036_u.bin",
        "32a1d8141af6897139a16b003a60bfaa57dc8f62569f7cac0a3afb22bb18b215",
    ),
    (
        "set0040_s.bin",
        "6223269b5ac785cecf7bef5951c5a351d1d94275ef84f2faf6d12cb920cee5aa",
    ),
    (
        "set0040_u.bin",
        "a589c2b1ac76438d6bbc78f1a0385b98ce0398a56f2fe01f21002f3e9913ff76",
    ),
    (
        "SET0042_S.BIN",
        "b1c9843ec3c8b9d15a82d42c84fab4ef983aed519b7f946ce2f02dd21cd3923c",
    ),
    (
        "set0042_u.bin",
        "42d0bd351a9202bce0f861c1f0608f78cbe4d465dbf88b98e7b419d0fbd6383a",
    ),
    (
        "set0043_s.bin",
        "5baa2ee069de8a3d27e8def2f399757f82b65f675b45bd3f320e0d94cd456345",
    ),
    (
        "set0043_u.bin",
        "a84cd792d533a62b0c092ce0098edc86135a77953e07adbfa9162c1bc2f51b88",
    ),
    (
        "set0044_s.bin",
        "f99456700d1cc7106bc63bfc4d453c2c639849c4ea32394f0064812f61144475",
    ),
    (
        "set0044_u.bin",
        "b4fa29db249188ec205efdd9f1d08206f766a55d6b6f6163495473d8050598c3",
    ),
    (
        "set_b_bigbogy_s.bin",
        "91488e4d4ea1516d98c7e3bbc987c866bb0e7ab3b21d6ae81cd129ed6e4be99e",
    ),
    (
        "set_b_bigbogy_u.bin",
        "f7130dc827b48f1359cf961bb897b46515a6050b953ef39c051e7733ea4e0952",
    ),
    (
        "set_b_bigfoot_s.bin",
        "a16d2810268a27ad6fa612c67706c69383fb8556100f2bf329b5dc125eeed192",
    ),
    (
        "set_b_bigfoot_u.bin",
        "3264a8914fb7a04189427cde5a6c71c8dadd2d72273c23eb93a0157c7ccb035f",
    ),
    (
        "set_b_fdog_s.bin",
        "66687aadf862bd776c8fc18b8e9f8e20089714856ee233b3902a591d0d5f2925",
    ),
    (
        "set_b_fdog_u.bin",
        "445050a7eb8e454489cbcc22134982000a343331147e1d53b557eac410dd52c8",
    ),
    (
        "set_b_Golem_e_s.bin",
        "a3cddd36d4005818b7708dd266087ad51ead5378f7f7d466ee66f70b00c90079",
    ),
    (
        "set_b_Golem_e_u.bin",
        "79dae081b960495b3626e2b54b15987532feea8c1a2d13984c2ddb730240952c",
    ),
    (
        "set_b_Golem_s.bin",
        "616a132dcba864a948fc21545e7364d4278ca02f75d9c99c300a9a332d6b862b",
    ),
    (
        "set_b_Golem_u.bin",
        "79dae081b960495b3626e2b54b15987532feea8c1a2d13984c2ddb730240952c",
    ),
    (
        "set_b_hotshot_s.bin",
        "3e0c1ec3e64ec282dc2d082d6140c2b4076e92808c3eb3dfbe116ab0f59e666c",
    ),
    (
        "set_b_hotshot_u.bin",
        "f7130dc827b48f1359cf961bb897b46515a6050b953ef39c051e7733ea4e0952",
    ),
    (
        "set_b_last1_s.bin",
        "bb9da82db2f91c2fd62dd4f5cbd1cdb93ed926f60ecc037525eb21227de41c1e",
    ),
    (
        "set_b_last1_u.bin",
        "0f57ae3ce25008197d051a78ee8c43a1a9d0800442e842d12e18b72b534a7ae7",
    ),
];
