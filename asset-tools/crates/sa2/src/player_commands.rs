use super::*;
use sa2_asset_tools::{
    death_zones, demo, event, event_camera, stage_catalog, stage_paths, stage_positions, story,
};
use std::ffi::{OsStr, OsString};

pub(super) const USAGE:&str="stage-bind SET OUTPUT.json EXE sa2lvl.ini ObjectLists.ini LEVEL_SECTION\nstage-positions EXE OUTPUT.json STAGE_ID\ndeath-zones EXE OUTPUT.json STAGE.ini\nstage-paths EXE OUTPUT.json STAGE.ini\nstory EXE OUTPUT.json ADVERTISE.ini\nevent-frame EVENT.prs OUTPUT.json FRAME\ndemo-frame RAW_DEMO.bin OUTPUT.json FRAME\ncampaign-frame EXE OUTPUT.json ADVERTISE.ini SEQUENCE ACTION_INDEX FRAME\ncampaign-camera EXE OUTPUT.json ADVERTISE.ini SEQUENCE ACTION_INDEX FRAME CAMERA_SLOT\nevent-camera EVENT.prs OUTPUT.json FRAME MOTION.bin CAMERA_SLOT";
pub(super) fn arity(cmd: &str) -> Option<usize> {
    match cmd {
        "campaign-camera" => Some(8),
        "event-camera" => Some(6),
        "stage-bind" | "campaign-frame" => Some(7),
        "stage-positions" | "death-zones" | "stage-paths" | "story" | "event-frame"
        | "demo-frame" => Some(4),
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
