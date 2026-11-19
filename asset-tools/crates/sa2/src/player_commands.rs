use super::*;
#[path = "story_run.rs"]
mod story_run;
use sa2_asset_tools::{
    death_zones, demo, event, event_camera, stage_catalog, stage_paths, stage_positions, story,
};
use std::ffi::{OsStr, OsString};

pub(super) const USAGE:&str="demo-prs-window DEMO.prs OUTPUT.json FIRST_RECORD COUNT\nfan-demos EXE OUTPUT.json PC_SA2_CONFIG_DIR\nfan-boss-rushes EXE OUTPUT.json PC_SA2_CONFIG_DIR\nstory-sections EXE OUTPUT.json SAVE_DATA_IMAGE\nstory-progress SAVE_DATA_IMAGE OUTPUT.json\nstage-result SAVE_DATA_IMAGE OUTPUT.json SLOT MISSION\nfan-rank EXE OUTPUT.json PC_SA2_CONFIG_DIR SEQUENCE ACTION_INDEX MISSION storedScore|seconds VALUE\nfan-emerald-contact EXE OUTPUT.json PC_SA2_CONFIG_DIR SEQUENCE ACTION_INDEX REQUEST.json\nfan-wild-canyon EXE OUTPUT.json PC_SA2_CONFIG_DIR SEQUENCE ACTION_INDEX BG_TASK_ALLOCATED(true|false)\nfan-big-foot EXE OUTPUT.json PC_SA2_CONFIG_DIR SEQUENCE ACTION_INDEX REQUEST.json\nfan-goal EXE OUTPUT.json PC_SA2_CONFIG_DIR SEQUENCE ACTION_INDEX LAYER PLACEMENT REQUEST.json\ndamage-ring MODULE OUTPUT.json LAND_SYMBOL REQUEST.json\nfan-replay EXE OUTPUT.json PC_SA2_CONFIG_DIR SEQUENCE ACTION_INDEX LAYER PLACEMENT REQUEST.json\nduel-platform EXE OUTPUT.json\nfan-tasks EXE OUTPUT.json PC_SA2_CONFIG_DIR\nmission-ranks EXE OUTPUT.json\nfan-stage EXE OUTPUT.json PC_SA2_CONFIG_DIR SEQUENCE ACTION_INDEX\nfan-levels EXE OUTPUT.json PC_SA2_CONFIG_DIR\nstage-raycast MODULE OUTPUT.json LAND_SYMBOL REQUEST.json\nstory-run EXE OUTPUT.json ADVERTISE.ini SEQUENCE\ncity-rider-pose STAGE_REL_OR_DATA_DLL OUTPUT.json EXE SONICMDL.prs ACTION FRAME\ncity-board-glb EXE OUTPUT.glb VARIANT OBJTEX_STG13.prs\ncity-board-model EXE OUTPUT.sa2mdl VARIANT\ncity-board-pose STAGE_REL_OR_DATA_DLL OUTPUT.json EXE VARIANT ACTION FRAME\ncity-collision STAGE_REL_OR_DATA_DLL OUTPUT.json [UNIFORM_SCALE]\ncity-raycast STAGE_REL_OR_DATA_DLL OUTPUT.json REQUEST.json\ncity-spawn-ground STAGE_REL_OR_DATA_DLL OUTPUT.json EXE MAX_DISTANCE\nstage20-splash stg20D.rel OUTPUT.json QUERY\nstage20-entry stg20D.rel OUTPUT.json REQUEST.json\nstage-fog FOG.bin OUTPUT.json\nstage-light-table STAGE_light.bin OUTPUT.json\nstage-light-base STAGE_light.bin OUTPUT.json\nmessage-bank MESSAGE.prs OUTPUT.json\nstage-registration STAGE.rel OUTPUT.json\ncity-board-resources stg13D.rel OUTPUT.json\nsonic-state-pose MTN.prs OUTPUT.json EXE SONICMDL.prs STATE FRAME\ncharacter-motion MTN.prs OUTPUT.json SLOT\ncharacter-motion-bank MTN.prs OUTPUT.json\ncharacter-animations EXE OUTPUT.json\nplayer-physics EXE OUTPUT.json\nstage-bind SET OUTPUT.json EXE sa2lvl.ini ObjectLists.ini LEVEL_SECTION\nstage-positions EXE OUTPUT.json STAGE_ID\ndeath-zones EXE OUTPUT.json STAGE.ini\nstage-paths EXE OUTPUT.json STAGE.ini\nstory EXE OUTPUT.json ADVERTISE.ini\nevent-frame EVENT.prs OUTPUT.json FRAME\ndemo-frame RAW_DEMO.bin OUTPUT.json FRAME\ncampaign-frame EXE OUTPUT.json ADVERTISE.ini SEQUENCE ACTION_INDEX FRAME\ncampaign-camera EXE OUTPUT.json ADVERTISE.ini SEQUENCE ACTION_INDEX FRAME CAMERA_SLOT\nevent-camera EVENT.prs OUTPUT.json FRAME MOTION.bin CAMERA_SLOT";
pub(super) fn arity(cmd: &str) -> Option<usize> {
    match cmd {
        "duel-platform"
        | "story-progress"
        | "mission-ranks"
        | "character-motion-bank"
        | "character-animations"
        | "player-physics"
        | "stage-fog"
        | "stage-light-table"
        | "stage-light-base"
        | "message-bank"
        | "stage-registration"
        | "city-board-resources" => Some(3),
        "fan-goal" | "fan-replay" | "fan-rank" => Some(9),
        "campaign-camera" => Some(8),
        "event-camera" | "fan-stage" => Some(6),
        "fan-emerald-contact"
        | "fan-wild-canyon"
        | "fan-big-foot"
        | "sonic-state-pose"
        | "city-rider-pose"
        | "city-board-pose"
        | "stage-bind"
        | "campaign-frame" => Some(7),
        "demo-prs-window" | "stage-result" | "damage-ring" | "stage-raycast" | "story-run"
        | "city-spawn-ground" => Some(5),
        "story-sections" | "stage-positions" | "death-zones" | "stage-paths" | "story"
        | "event-frame" | "demo-frame" | "fan-demos" | "fan-boss-rushes" | "fan-levels"
        | "character-motion" | "fan-tasks" | "stage20-splash" | "stage20-entry" => Some(4),
        _ => None,
    }
}
struct Extra {
    path: PathBuf,
    logical: String,
    bytes: Vec<u8>,
    role: &'static str,
}
impl Extra {
    fn read(path: &Path, logical: String, limit: u64, role: &'static str) -> Result<Self> {
        Ok(Self {
            path: path.into(),
            logical,
            bytes: read_source_limited(path, limit)?,
            role,
        })
    }
    fn mapping(path: &OsStr, role: &'static str) -> Result<Self> {
        let path = Path::new(path);
        Self::read(
            path,
            format!("mappings/{}", basename(path)?),
            2 * 1024 * 1024,
            role,
        )
    }
    fn text(&self) -> Result<&str> {
        std::str::from_utf8(&self.bytes).map_err(|_| Error::invalid("mapping must be UTF-8"))
    }
}
fn text_arg(s: &OsStr) -> Result<&str> {
    s.to_str()
        .ok_or_else(|| Error::invalid("invalid text argument"))
}
fn number<T: std::str::FromStr>(s: &OsStr) -> Result<T> {
    text_arg(s)?
        .parse()
        .map_err(|_| Error::invalid("expected an unsigned number"))
}
fn scene_packet(event: &event::Event, frame: u32) -> Result<Value> {
    let at = event.frame(frame)?;
    Ok(
        json!({"timing":at,"totalFrames":event.scenes[0].frames,"common":event.scenes[0],"cut":event.scenes[at.scene],
        "scope":"Source scene selection. Model references and motion IDs are not rendered pixels or mixed audio."}),
    )
}
fn camera_packet(event: &event::Event, frame: u32, bank: &[u8], slot: usize) -> Result<Value> {
    let at = event.frame(frame)?;
    let index = *event.scenes[at.scene]
        .camera_motions
        .get(slot)
        .ok_or_else(|| Error::invalid("camera slot outside selected event scene"))?;
    let sample = event_camera::sample(bank, index, at.scene_frame as f32)?;
    Ok(json!({"timing":at,"cameraSlot":slot,"motionIndex":index,"camera":sample}))
}

/// A read-only preview cursor can seek across content requests. It never attests to mission completion.
fn campaign(
    exe: &[u8],
    exe_path: &Path,
    extras: &mut Vec<Extra>,
    args: &[OsString],
) -> Result<Value> {
    let sequence_name = text_arg(&args[4])?;
    let ordinal: usize = number(&args[5])?;
    let frame: u32 = number(&args[6])?;
    let sequences = story::read(exe, extras[0].text()?)?;
    let sequence = sequences
        .get(sequence_name)
        .ok_or_else(|| Error::invalid("unknown story sequence section"))?;
    let mut cursor = story::Cursor::new(sequence);
    for _ in 0..ordinal {
        let prev = cursor
            .dispatch()
            .ok_or_else(|| Error::invalid("action outside authored sequence"))?;
        cursor.acknowledge(&prev.token)?;
    }
    let dispatch = cursor
        .dispatch()
        .ok_or_else(|| Error::invalid("action outside authored sequence"))?;
    let install = exe_path
        .parent()
        .ok_or_else(|| Error::invalid("expected game installation executable"))?;
    let load = |extras: &mut Vec<Extra>, logical: String, role| -> Result<usize> {
        let path = install.join(&logical);
        let extra = Extra::read(&path, logical, 64 * 1024 * 1024, role)?;
        extras.push(extra);
        Ok(extras.len() - 1)
    };
    let payload = match dispatch.action {
        story::Action::Event { event: id } => {
            let logical = format!("resource/gd_PC/event/e{id:04}.prs");
            if install.join(&logical).try_exists()? {
                let i = load(extras, logical, "story-event-image")?;
                let clip = event::read_prs(&extras[i].bytes)?;
                if let Some(slot) = args.get(7) {
                    let m = load(
                        extras,
                        format!("resource/gd_PC/event/e{id:04}motion.bin"),
                        "event-motion-bank",
                    )?;
                    camera_packet(&clip, frame, &extras[m].bytes, number(slot)?)?
                } else {
                    scene_packet(&clip, frame)?
                }
            } else {
                json!({"available":false,"missingMainEvent":logical,"reason":"No matching main event PRS; movie or special-event dispatch is not implemented."})
            }
        }
        story::Action::Level { level, .. } => {
            if args.len() == 8 {
                return Err(Error::invalid(
                    "selected story action is a level, not an event camera",
                ));
            }
            if frame != 0 {
                return Err(Error::invalid("level preview supports entry only; movement and completion are not implemented"));
            }
            let mut layers = Vec::new();
            if level == 13 {
                for file in ["set0013_s.bin", "set0013_u.bin"] {
                    let i = load(
                        extras,
                        format!("resource/gd_PC/{file}"),
                        "normal-city-layout",
                    )?;
                    let layout = CityLayout::load(&extras[i].bytes, exe)?;
                    layers.push(json!({"source":layout.source_layer,"placements":layout.placements.len(),"partialCallbacks":layout.callback_counts,"unsupportedPlacements":layout.unsupported_placements}));
                }
            }
            json!({"entryPoses":stage_positions::for_stage(exe,level as u16)?,"normalCityLayers":layers,
                "scope":"Authored character poses and available layer bindings. No character dynamics, mission execution or stage-clear transition."})
        }
        story::Action::Credits => {
            if args.len() == 8 {
                return Err(Error::invalid("credits do not select an event camera"));
            }
            if frame != 0 {
                return Err(Error::invalid("credits animation unavailable"));
            }
            json!({"available":false,"reason":"Credits rendering not implemented."})
        }
    };
    Ok(
        json!({"sequence":sequence_name,"actionIndex":ordinal,"action":dispatch.action,"entryRva":dispatch.source_rva,"content":payload,"mode":"offline content preview; no campaign save or unlocks"}),
    )
}
pub(super) fn run(command: &str, args: &[OsString], root: Option<&Path>) -> Result<()> {
    let input = Path::new(&args[1]);
    let output = Path::new(&args[2]);
    local_output(output)?;
    let bytes = read_source_limited(input, 128 * 1024 * 1024)?;
    let label = source_label(input, root)?;
    let mut extras = Vec::new();
    let doc = match command {
        "stage20-entry" => {
            let path = Path::new(&args[3]);
            let req = Extra::read(
                path,
                basename(path)?.to_owned(),
                1024 * 1024,
                "entry-observations",
            )?;
            let result = sa2_asset_tools::stage20::replay(&bytes, &req.bytes)?;
            extras.push(req);
            result
        }
        "duel-platform" => serde_json::to_value(sa2_asset_tools::duel_platform::read(&bytes)?)?,
        "story-sections" => {
            let p = Path::new(&args[3]);
            let snapshot = Extra::read(
                p,
                format!("snapshots/{}", basename(p)?),
                128 * 1024 * 1024,
                "offline-save-data-image",
            )?;
            let result = story::saved_sections(&bytes, &snapshot.bytes)?;
            extras.push(snapshot);
            result
        }
        "story-progress" => {
            serde_json::to_value(sa2_asset_tools::stage_result::story_progress(&bytes)?)?
        }
        "stage-result" => serde_json::to_value(sa2_asset_tools::stage_result::read(
            &bytes,
            number(&args[3])?,
            number(&args[4])?,
        )?)?,
        "mission-ranks" => serde_json::to_value(sa2_asset_tools::mission_ranks::read(&bytes)?)?,
        "fan-emerald-contact"
        | "fan-wild-canyon"
        | "fan-big-foot"
        | "fan-rank"
        | "fan-stage"
        | "fan-demos"
        | "fan-boss-rushes"
        | "fan-levels"
        | "fan-tasks"
        | "fan-replay"
        | "fan-goal" => {
            let cfg = Path::new(&args[3]);
            for (name, role) in [
                ("ADVERTISE.ini", "story-mapping"),
                ("sa2lvl.ini", "level-mapping"),
                ("ObjectLists.ini", "object-list-mapping"),
            ] {
                extras.push(Extra::mapping(cfg.join(name).as_os_str(), role)?);
            }
            let catalog = sa2_asset_tools::fan_stage::Campaign::new(
                &bytes,
                extras[0].text()?,
                extras[1].text()?,
                extras[2].text()?,
            )?;
            let gd = input
                .parent()
                .ok_or_else(|| Error::invalid("missing install root"))?
                .join("resource/gd_PC");
            if command == "fan-tasks" {
                let mut cached = std::collections::BTreeMap::<String, usize>::new();
                let coverage = sa2_asset_tools::task_coverage::read(&catalog, |name| {
                    if let Some(&index) = cached.get(name) {
                        return Ok(Some(extras[index].bytes.clone()));
                    }
                    let path = gd.join(name);
                    if !path.try_exists()? {
                        return Ok(None);
                    }
                    let src = Extra::read(
                        &path,
                        format!("resource/gd_PC/{name}"),
                        2 * 1024 * 1024,
                        "campaign-set",
                    )?;
                    cached.insert(name.into(), extras.len());
                    let data = src.bytes.clone();
                    extras.push(src);
                    Ok(Some(data))
                })?;
                serde_json::to_value(coverage)?
            } else if command == "fan-rank" {
                use sa2_asset_tools::mission_ranks::Observation;
                let mission: u8 = number(&args[6])?;
                let sequence = text_arg(&args[4])?;
                let ordinal: usize = number(&args[5])?;
                let dispatch = catalog
                    .dispatch(sequence, ordinal)
                    .ok_or_else(|| Error::invalid("not an authored campaign dispatch"))?;
                let row = catalog
                    .rank_rows(dispatch)
                    .into_iter()
                    .find(|r| r.mission == mission)
                    .ok_or_else(|| {
                        Error::invalid("no authored rank row for this dispatch and mission")
                    })?;
                let observation = match text_arg(&args[7])? {
                    "storedScore" => Observation::StoredScore(number(&args[8])?),
                    "seconds" => Observation::Seconds(number(&args[8])?),
                    _ => return Err(Error::invalid("expected storedScore or seconds")),
                };
                json!({"dispatch":dispatch,"row":row,"observedMinusThresholdDThroughA":row.differences(observation)?,"scope":"Comparison in authored units; does not award a rank or commit a clear."})
            } else if command == "fan-demos" {
                serde_json::to_value(catalog.demos())?
            } else if command == "fan-boss-rushes" {
                serde_json::to_value(catalog.boss_rushes())?
            } else if command == "fan-levels" {
                serde_json::to_value(catalog.levels())?
            } else {
                let stage = catalog.mount(text_arg(&args[4])?, number(&args[5])?, |name| {
                    let logical = format!("resource/gd_PC/{name}");
                    if let Some(source) = extras.iter().find(|source| source.logical == logical) {
                        return Ok(Some(source.bytes.clone()));
                    }
                    let path = gd.join(name);
                    if !path.try_exists()? {
                        return Ok(None);
                    }
                    let src = Extra::read(&path, logical, 64 * 1024 * 1024, "mounted-resource")?;
                    let data = src.bytes.clone();
                    extras.push(src);
                    Ok(Some(data))
                })?;
                if matches!(command, "fan-wild-canyon" | "fan-emerald-contact") {
                    if stage.dispatch.level != 16 {
                        return Err(Error::invalid("expected Wild Canyon dispatch"));
                    }
                    let module = extras
                        .iter()
                        .find(|e| e.logical == "resource/gd_PC/stg16D.rel")
                        .ok_or_else(|| Error::invalid("missing mounted Wild Canyon REL"))?;
                    if command == "fan-emerald-contact" {
                        let path = Path::new(&args[6]);
                        let request = Extra::read(
                            path,
                            format!("requests/{}", basename(path)?),
                            1024 * 1024,
                            "emerald-contact-request",
                        )?;
                        let report = sa2_asset_tools::wild_canyon::contacts(
                            &module.bytes,
                            &stage,
                            &request.bytes,
                        )?;
                        extras.push(request);
                        json!({"dispatch":stage.dispatch,"contacts":report})
                    } else {
                        let task = sa2_asset_tools::wild_canyon::enter(
                            &module.bytes,
                            number(&args[6])?,
                            &stage,
                        )?;
                        let light = task["lightFile"]
                            .as_str()
                            .ok_or_else(|| Error::invalid("missing lighting filename"))?;
                        let stem = light
                            .strip_suffix(".bin")
                            .ok_or_else(|| Error::invalid("unexpected lighting filename"))?;
                        let suffix = task["gcSuffix"]
                            .as_str()
                            .ok_or_else(|| Error::invalid("missing lighting suffix"))?;
                        let gc_name = format!("{stem}{suffix}");
                        let read = |name: &str| {
                            Extra::read(
                                &gd.join(name),
                                format!("resource/gd_PC/{name}"),
                                1024,
                                "stage-lighting",
                            )
                        };
                        let src = read(light)?;
                        let gc = read(&gc_name)?;
                        let lights =
                            sa2_asset_tools::wild_canyon::light_tables(&src.bytes, &gc.bytes)?;

                        let templates = sa2_asset_tools::wild_canyon::pc_collision_templates(
                            &bytes,
                            &module.bytes,
                        )?;
                        extras.extend([src, gc]);
                        json!({"dispatch":stage.dispatch,"stageTask":task,"lighting":lights,"pcCollisionTemplates":templates})
                    }
                } else if command == "fan-big-foot" {
                    if stage.dispatch.level != 60 {
                        return Err(Error::invalid("expected Big Foot story dispatch"));
                    }
                    let module = extras
                        .iter()
                        .find(|e| e.logical == "resource/gd_PC/boss_bigfootD.rel")
                        .ok_or_else(|| Error::invalid("missing mounted Big Foot REL"))?;
                    let p = Path::new(&args[6]);
                    let request = Extra::read(
                        p,
                        format!("requests/{}", basename(p)?),
                        1024 * 1024,
                        "boss-stage-request",
                    )?;
                    let replay = sa2_asset_tools::big_foot::replay(&module.bytes, &request.bytes)?;
                    extras.push(request);
                    json!({"dispatch":stage.dispatch,"replay":replay})
                } else if matches!(command, "fan-replay" | "fan-goal") {
                    let p = Path::new(&args[8]);
                    let request = Extra::read(
                        p,
                        format!("requests/{}", basename(p)?),
                        1024 * 1024,
                        "object-replay-request",
                    )?;
                    let layer: usize = number(&args[6])?;
                    let row: usize = number(&args[7])?;
                    let replay = if command == "fan-goal" {
                        let name = format!("resource/gd_PC/stg{:02}D.rel", stage.dispatch.level);
                        let module = extras
                            .iter()
                            .find(|e| e.logical == name)
                            .ok_or_else(|| Error::invalid("missing mounted goal module"))?;
                        let effects = Extra::read(
                            &gd.join("efftex_common.prs"),
                            "resource/gd_PC/efftex_common.prs".into(),
                            32 * 1024 * 1024,
                            "goal-effect-textures",
                        )?;
                        let result = sa2_asset_tools::goal_ring::replay(
                            &stage,
                            layer,
                            row,
                            &module.bytes,
                            &bytes,
                            &effects.bytes,
                            &request.bytes,
                        )?;
                        extras.push(effects);
                        result
                    } else {
                        stage.replay_object(layer, row, &request.bytes)?
                    };
                    extras.push(request);
                    json!({"dispatch":stage.dispatch,"layer":stage.layers[layer].filename,"placementIndex":row,"replay":replay,"scope":"offline native callback with supplied engine observations"})
                } else {
                    serde_json::to_value(stage)?
                }
            }
        }
        "damage-ring" => {
            let p = Path::new(&args[4]);
            let src = Extra::read(
                p,
                format!("requests/{}", basename(p)?),
                64 * 1024,
                "damage-ring-request",
            )?;
            let stage = sa2_asset_tools::stage_land::read_named(&bytes, text_arg(&args[3])?)?;
            let doc = sa2_asset_tools::damage_ring_stage::replay(&stage, &src.bytes)?;
            extras.push(src);
            doc
        }
        "stage-raycast" => {
            let p = Path::new(&args[4]);
            let src = Extra::read(
                p,
                format!("requests/{}", basename(p)?),
                64 * 1024,
                "ray-request",
            )?;
            let req: RayRequest = serde_json::from_slice(&src.bytes)?;
            let symbol = text_arg(&args[3])?;
            let stage = sa2_asset_tools::stage_land::read_named(&bytes, symbol)?;
            let world = collision::bake_stage(&stage, req.uniform_scale)?;
            let hit = world.raycast(req.origin, req.direction, req.max_distance)?;
            extras.push(src);
            json!({"symbol":symbol,"request":req,"hit":hit,"bounds":world.bounds,"scope":"static geometric query"})
        }
        "story-run" => {
            extras.push(Extra::mapping(&args[3], "native-table-mapping")?);
            story_run::run(
                &bytes,
                input
                    .parent()
                    .ok_or_else(|| Error::invalid("missing install directory"))?,
                &mut extras,
                text_arg(&args[4])?,
            )?
        }
        "sonic-state-pose" | "city-rider-pose" => {
            for (arg, role) in [(3, "pc-executable"), (4, "sonic-rider-model")] {
                let path = Path::new(&args[arg]);
                extras.push(Extra::read(
                    path,
                    source_label(path, root)?,
                    128 * 1024 * 1024,
                    role,
                )?);
            }
            let frame = finite_argument(&args[6])?;
            if command == "sonic-state-pose" {
                sa2_asset_tools::character_motions::sonic_pose(
                    &bytes,
                    &extras[0].bytes,
                    &extras[1].bytes,
                    number(&args[5])?,
                    frame,
                )?
            } else {
                sa2_asset_tools::city_rider::sample(
                    &bytes,
                    &extras[0].bytes,
                    &extras[1].bytes,
                    number(&args[5])?,
                    frame,
                )?
            }
        }
        "city-board-pose" | "city-spawn-ground" => {
            let exe = Path::new(&args[3]);
            extras.push(Extra::read(
                exe,
                source_label(exe, root)?,
                128 * 1024 * 1024,
                "pc-executable",
            )?);
            let exe = &extras[0].bytes;
            if command == "city-board-pose" {
                let variant = board_variant(&args[4])?;
                let action: i16 = number(&args[5])?;
                let frame = finite_argument(&args[6])?;
                let (bank, address_space) = if bytes.starts_with(b"MZ") {
                    (sa2_asset_tools::board_motion::read_pc(&bytes)?, "pe_rva")
                } else {
                    let rel = sa2_asset_tools::city_rel::CityRel::read(&bytes)?;
                    (sa2_asset_tools::board_motion::read(&rel)?, "rel_section_5")
                };
                let motion = bank
                    .for_action(action)
                    .ok_or_else(|| Error::invalid("no stage board default for action"))?;
                if frame < 0. || frame >= f64::from(motion.frame_count) {
                    return Err(Error::invalid("board frame outside motion duration"));
                }
                let native = sa2_asset_tools::board_model::extract(exe, variant)?;
                let source_model = model::read_model(&native.sa2mdl)?;
                let sample = pose::evaluate_model_pose(&source_model, Some(motion), frame)?;
                json!({"variant":variant,"riderAction":action,"defaultMotionSlot":action+24,"motionAddressSpace":address_space,"motionOffset":motion.offset,
                    "textureNames":native.texture_names,"pose":sample,"scope":"Board-local pose from the original stage default motion; live replacement slots and rider/world transform are external."})
            } else {
                let distance = finite_argument(&args[4])?;
                let row = stage_positions::for_stage(exe, 13)?
                    .into_iter()
                    .find(|r| {
                        r.character == "Sonic" && r.kind == stage_positions::PositionKind::Start
                    })
                    .ok_or_else(|| Error::invalid("missing Sonic City Escape start row"))?;
                let world = collision::bake_stage(&city_collision(&bytes)?, 1.)?;
                let mut probes = Vec::new();
                for p in &row.poses {
                    let hit = world.raycast(p.position.map(f64::from), [0., -1., 0.], distance)?;
                    probes.push(json!({"pose":p,"hit":hit}));
                }
                json!({"stage":13,"character":row.character,"startRowVa":row.row_va,"maxDistance":distance,"probes":probes,
                    "scope":"Downward rays from the three authored PC start variants against the supplied module's static collision; these are not observed player contacts."})
            }
        }
        "stage20-splash" => {
            let (name, count) =
                sa2_asset_tools::stage20::splash_resource(&bytes, number(&args[3])?)?;
            let path = input.with_file_name(&name);
            let archive = Extra::read(
                &path,
                source_label(&path, root)?,
                16 * 1024 * 1024,
                "splash-textures",
            )?;
            let report = sa2_asset_tools::stage20::splash_textures(&archive.bytes, count)?;
            extras.push(archive);
            report
        }
        "stage-fog" => sa2_asset_tools::stage_fog::read(&bytes)?,
        "stage-light-table" => {
            let filename = basename(input)?;
            if filename.ends_with("_gc.bin") {
                sa2_asset_tools::stage_lighting::resolve(None, Some(&bytes))?
            } else {
                let stem = filename
                    .strip_suffix(".bin")
                    .ok_or_else(|| Error::invalid("expected lighting .bin filename"))?;
                let path = input.with_file_name(format!("{stem}_gc.bin"));
                if path.try_exists()? {
                    extras.push(Extra::read(
                        &path,
                        source_label(&path, root)?,
                        576,
                        "stage-light-override",
                    )?);
                }
                sa2_asset_tools::stage_lighting::resolve(
                    Some(&bytes),
                    extras.last().map(|e| e.bytes.as_slice()),
                )?
            }
        }
        "stage-light-base" => {
            json!({"derivedWords":sa2_asset_tools::stage_lighting::derive(&bytes)?})
        }
        "message-bank" => sa2_asset_tools::message_bank::read_prs(&bytes)?,
        "stage-registration" => sa2_asset_tools::city_rel::stage_registration(&bytes)?,
        "city-board-resources" => {
            let rel = sa2_asset_tools::city_rel::CityRel::read(&bytes)?;
            json!({"dataSection":5,"resources":rel.board_resources()?})
        }
        "character-motion-bank" => {
            serde_json::to_value(sa2_asset_tools::character_motions::Bank::read(&bytes)?.rows)?
        }
        "character-motion" => serde_json::to_value(
            sa2_asset_tools::character_motions::Bank::read(&bytes)?.motion(number(&args[3])?)?,
        )?,
        "character-animations" => {
            serde_json::to_value(sa2_asset_tools::character_animations::read(&bytes)?)?
        }
        "player-physics" => serde_json::to_value(sa2_asset_tools::player_physics::read(&bytes)?)?,
        "stage-bind" => {
            let exe = Path::new(&args[3]);
            extras.push(Extra::read(
                exe,
                source_label(exe, root)?,
                128 * 1024 * 1024,
                "object-executable",
            )?);
            extras.push(Extra::mapping(&args[4], "level-mapping")?);
            extras.push(Extra::mapping(&args[5], "object-list-mapping")?);
            serde_json::to_value(stage_catalog::bind(
                &bytes,
                &extras[0].bytes,
                extras[1].text()?,
                extras[2].text()?,
                text_arg(&args[6])?,
            )?)?
        }
        "stage-positions" => {
            serde_json::to_value(stage_positions::for_stage(&bytes, number(&args[3])?)?)?
        }
        "death-zones" | "stage-paths" | "story" | "campaign-frame" | "campaign-camera" => {
            extras.push(Extra::mapping(&args[3], "native-table-mapping")?);
            match command {
                "death-zones" => {
                    serde_json::to_value(death_zones::read(&bytes, extras[0].text()?)?)?
                }
                "stage-paths" => {
                    serde_json::to_value(stage_paths::read(&bytes, extras[0].text()?)?)?
                }
                "story" => serde_json::to_value(story::read(&bytes, extras[0].text()?)?)?,
                _ => campaign(&bytes, input, &mut extras, args)?,
            }
        }
        "demo-prs-window" => serde_json::to_value(demo::prs_window(
            &bytes,
            number(&args[3])?,
            number(&args[4])?,
        )?)?,
        "demo-frame" => {
            let demo = demo::read_raw(&bytes)?;
            let index: usize = number(&args[3])?;
            json!({"frameIndex":index,"physicalRecordCount":demo.len(),"input":demo.frame(index).ok_or_else(||Error::invalid("frame outside raw demo"))?})
        }
        "event-frame" => scene_packet(&event::read_prs(&bytes)?, number(&args[3])?)?,
        "event-camera" => {
            let path = Path::new(&args[4]);
            extras.push(Extra::read(
                path,
                source_label(path, root)?,
                64 * 1024 * 1024,
                "event-motion-bank",
            )?);
            camera_packet(
                &event::read_prs(&bytes)?,
                number(&args[3])?,
                &extras[0].bytes,
                number(&args[5])?,
            )?
        }
        _ => return Err(Error::invalid("unknown stage command")),
    };
    let deps = extras
        .iter()
        .map(|e| RuntimeDependency {
            path: &e.path,
            bytes: &e.bytes,
            role: e.role,
            logical_path: e.logical.clone(),
        })
        .collect::<Vec<_>>();
    save_runtime_document(
        input,
        &label,
        &bytes,
        output,
        &doc,
        &format!("{command}-json"),
        &deps,
    )
}
