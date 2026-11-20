use super::*;
#[path = "story_run.rs"]
mod story_run;
use sa2_asset_tools::{
    death_zones, demo, event, event_camera, stage_catalog, stage_paths, stage_positions, story,
};
use std::ffi::{OsStr, OsString};

pub(super) const USAGE:&str="demo-prs-window DEMO.prs OUTPUT.json FIRST_RECORD COUNT\nfan-demos EXE OUTPUT.json PC_SA2_CONFIG_DIR\nfan-boss-rushes EXE OUTPUT.json PC_SA2_CONFIG_DIR\nstory-sections EXE OUTPUT.json SAVE_DATA_IMAGE\nstory-progress SAVE_DATA_IMAGE OUTPUT.json\nstage-result SAVE_DATA_IMAGE OUTPUT.json SLOT MISSION\nfan-rank EXE OUTPUT.json PC_SA2_CONFIG_DIR SEQUENCE ACTION_INDEX MISSION storedScore|seconds VALUE\nfan-emerald-contact EXE OUTPUT.json PC_SA2_CONFIG_DIR SEQUENCE ACTION_INDEX REQUEST.json\nfan-wild-canyon EXE OUTPUT.json PC_SA2_CONFIG_DIR SEQUENCE ACTION_INDEX BG_TASK_ALLOCATED(true|false)\nfan-big-foot EXE OUTPUT.json PC_SA2_CONFIG_DIR SEQUENCE ACTION_INDEX REQUEST.json\nfan-goal EXE OUTPUT.json PC_SA2_CONFIG_DIR SEQUENCE ACTION_INDEX LAYER PLACEMENT REQUEST.json\ndamage-ring MODULE OUTPUT.json LAND_SYMBOL REQUEST.json\nfan-replay EXE OUTPUT.json PC_SA2_CONFIG_DIR SEQUENCE ACTION_INDEX LAYER PLACEMENT REQUEST.json\nduel-platform EXE OUTPUT.json\nfan-tasks EXE OUTPUT.json PC_SA2_CONFIG_DIR\nmission-ranks EXE OUTPUT.json\nfan-stage EXE OUTPUT.json PC_SA2_CONFIG_DIR SEQUENCE ACTION_INDEX\nfan-starts EXE OUTPUT.json PC_SA2_CONFIG_DIR\nfan-levels EXE OUTPUT.json PC_SA2_CONFIG_DIR\nkart-finish-display CartD.rel OUTPUT.json resultCart.prs INPUT.json\nkart-drift-step CartD.rel OUTPUT.json INPUT.json\nkart-drift-windup CartD.rel OUTPUT.json INPUT.json\nkart-scripted-motion CartD.rel OUTPUT.json INPUT.json\nkart-active-profile CartD.rel OUTPUT.json INPUT.json\nkart-vehicle-init CartD.rel OUTPUT.json INPUT.json\nkart-spin-drive CartD.rel OUTPUT.json INPUT.json\nkart-recovery-drive CartD.rel OUTPUT.json INPUT.json\nkart-buffer-tuning CartD.rel OUTPUT.json PRE_RELOCATION_BUFFER.bin ENGINE_WORD_B4\nkart-race-tuning CartD.rel OUTPUT.json INPUT.json\nkart-guided-drive CartD.rel OUTPUT.json INPUT.json\nkart-contact-step CartD.rel OUTPUT.json INPUT.json\nkart-normal-contact CartD.rel OUTPUT.json EXE COURSE INPUT.json\nkart-contact-recovery CartD.rel OUTPUT.json EXE COURSE INPUT.json\nkart-course-recovery CartD.rel OUTPUT.json EXE COURSE INPUT.json\nkart-fall-recovery CartD.rel OUTPUT.json INPUT.json\nkart-ring-particle CartD.rel OUTPUT.json INPUT.json\nkart-balloon-display CartD.rel OUTPUT.json INPUT.json\nkart-dash-display CartD.rel OUTPUT.json INPUT.json\nkart-ring-display CartD.rel OUTPUT.json INPUT.json\nkart-ring-pickup CartD.rel OUTPUT.json INPUT.json\nkart-driving-audio CartD.rel OUTPUT.json INPUT.json\nkart-update-gate CartD.rel OUTPUT.json INPUT.json\nkart-drive-input CartD.rel OUTPUT.json INPUT.json\nkart-timeout CartD.rel OUTPUT.json INPUT.json\nkart-pursuit-clock CartD.rel OUTPUT.json INPUT.json\nkart-target-model CartD.rel OUTPUT.json MODE\nkart-target-textures CartD.rel OUTPUT.json MODE\nkart-target-display CartD.rel OUTPUT.json INPUT.json\nkart-story-target CartD.rel OUTPUT.json INPUT.json\nkart-finish-menu CartD.rel OUTPUT.json INPUT.json\nkart-field-update CartD.rel OUTPUT.json FIELD.json\nkart-standings CartD.rel OUTPUT.json OCCUPIED_SLOTS.json\nkart-crossing-directory CartD.rel OUTPUT.json EXE\nkart-segment-geometry CartD.rel OUTPUT.json OBSERVATIONS.json\nkart-progress-tick CartD.rel OUTPUT.json OBSERVATIONS.json\nkart-race-gate-tick CartD.rel OUTPUT.json OBSERVATIONS.json\nkart-time-extension CartD.rel OUTPUT.json\nkart-story-gate-display CartD.rel OUTPUT.json INPUT.json\nkart-gate-tick CartD.rel OUTPUT.json OBSERVATIONS.json\nkart-ramp-response CartD.rel OUTPUT.json RESPONSE_INPUT.json\nkart-dash-panel CartD.rel OUTPUT.json INPUT.json\nkart-ramp-tick CartD.rel OUTPUT.json OBSERVATIONS.json\nkart-ramp-init CartD.rel OUTPUT.json TASK_POSE.json\nkart-story-gate-sequence CartD.rel OUTPUT.json EXE SET CONTACTS.json\nkart-story-gates CartD.rel OUTPUT.json EXE SET\nkart-balloon-layout CartD.rel OUTPUT.json EXE SET\nkart-ring-groups CartD.rel OUTPUT.json EXE SET\nkart-objects SET OUTPUT.json EXE\nkart-vehicles EXE OUTPUT.json\nkart-courses EXE OUTPUT.json\nkart-piece-collision EXE OUTPUT.json DATA_DLL COURSE CASES.json\nkart-native-layout EXE OUTPUT.json DATA_DLL COURSE\nkart-course-billboards EXE OUTPUT.json DATA_DLL CartD.rel COURSE RANDS.json\nkart-course-background EXE OUTPUT.json DATA_DLL CartD.rel COURSE FRAMES.json\nkart-course-materials EXE OUTPUT.json DATA_DLL CartD.rel course.pak COURSE\nkart-raycast EXE OUTPUT.json DATA_DLL COURSE(101|280|beginner|standard|expert) REQUEST.json\nstage-raycast MODULE OUTPUT.json LAND_SYMBOL REQUEST.json\nstory-run EXE OUTPUT.json ADVERTISE.ini SEQUENCE\ncity-rider-pose STAGE_REL_OR_DATA_DLL OUTPUT.json EXE SONICMDL.prs ACTION FRAME\ncity-board-glb EXE OUTPUT.glb VARIANT OBJTEX_STG13.prs\ncity-board-model EXE OUTPUT.sa2mdl VARIANT\ncity-board-pose STAGE_REL_OR_DATA_DLL OUTPUT.json EXE VARIANT ACTION FRAME\ncity-collision STAGE_REL_OR_DATA_DLL OUTPUT.json [UNIFORM_SCALE]\ncity-raycast STAGE_REL_OR_DATA_DLL OUTPUT.json REQUEST.json\ncity-spawn-ground STAGE_REL_OR_DATA_DLL OUTPUT.json EXE MAX_DISTANCE\nstage20-splash stg20D.rel OUTPUT.json QUERY\nstage20-entry stg20D.rel OUTPUT.json REQUEST.json\nstage-fog FOG.bin OUTPUT.json\nstage-light-table STAGE_light.bin OUTPUT.json\nstage-light-base STAGE_light.bin OUTPUT.json\nmessage-bank MESSAGE.prs OUTPUT.json\nstage-registration STAGE.rel OUTPUT.json\ncity-board-resources stg13D.rel OUTPUT.json\ncharacter-state-pose MTN.prs OUTPUT.json EXE MDL.prs STATE FRAME\nsonic-state-pose MTN.prs OUTPUT.json EXE SONICMDL.prs STATE FRAME\ncharacter-camera MTN.prs OUTPUT.json SLOT FRAME\ncharacter-motion MTN.prs OUTPUT.json SLOT\ncharacter-motion-bank MTN.prs OUTPUT.json\ncharacter-animations EXE OUTPUT.json\nplayer-physics EXE OUTPUT.json\nstage-bind SET OUTPUT.json EXE sa2lvl.ini ObjectLists.ini LEVEL_SECTION\nstage-positions EXE OUTPUT.json STAGE_ID\ndeath-zones EXE OUTPUT.json STAGE.ini\nstage-paths EXE OUTPUT.json STAGE.ini\nstory EXE OUTPUT.json ADVERTISE.ini\nevent-frame EVENT.prs OUTPUT.json FRAME\ndemo-frame RAW_DEMO.bin OUTPUT.json FRAME\ncampaign-frame EXE OUTPUT.json ADVERTISE.ini SEQUENCE ACTION_INDEX FRAME\ncampaign-camera EXE OUTPUT.json ADVERTISE.ini SEQUENCE ACTION_INDEX FRAME CAMERA_SLOT\nevent-camera EVENT.prs OUTPUT.json FRAME MOTION.bin CAMERA_SLOT\nfan-emerald-contact EXE OUTPUT.json PC_SA2_CONFIG_DIR SEQUENCE ACTION_INDEX REQUEST.json\nfan-wild-canyon EXE OUTPUT.json PC_SA2_CONFIG_DIR SEQUENCE ACTION_INDEX BG_TASK_ALLOCATED(true|false)\nfan-big-foot EXE OUTPUT.json PC_SA2_CONFIG_DIR SEQUENCE ACTION_INDEX REQUEST.json\nfan-goal EXE OUTPUT.json PC_SA2_CONFIG_DIR SEQUENCE ACTION_INDEX LAYER PLACEMENT REQUEST.json\ndamage-ring MODULE OUTPUT.json LAND_SYMBOL REQUEST.json\nfan-replay EXE OUTPUT.json PC_SA2_CONFIG_DIR SEQUENCE ACTION_INDEX LAYER PLACEMENT REQUEST.json\nduel-platform EXE OUTPUT.json\nduel-generator stg42D.rel OUTPUT.json REQUEST.json\nduel-platform-update stg42D.rel OUTPUT.json REQUEST.json\nduel-meteor stg42D.rel OUTPUT.json REQUEST.json\nduel-debris stg42D.rel OUTPUT.json REQUEST.json\nduel-dust stg42D.rel OUTPUT.json REQUEST.json\nduel-stage-frame stg42D.rel OUTPUT.json REQUEST.json\nduel-stage-setup stg42D.rel OUTPUT.json REQUEST.json\nduel-fog stg42D.rel OUTPUT.json REQUEST.json\nduel-director stg42D.rel OUTPUT.json REQUEST.json\nduel-gauge stg42D.rel OUTPUT.json REQUEST.json\nduel-title stg42D.rel OUTPUT.json REQUEST.json\nduel-display stg42D.rel OUTPUT.json REQUEST.json\nfan-tasks EXE OUTPUT.json PC_SA2_CONFIG_DIR\nmission-ranks EXE OUTPUT.json\nfan-stage EXE OUTPUT.json PC_SA2_CONFIG_DIR SEQUENCE ACTION_INDEX\nfan-stage-reuse EXE OUTPUT.json PC_SA2_CONFIG_DIR\nfan-levels EXE OUTPUT.json PC_SA2_CONFIG_DIR\nstage-raycast MODULE OUTPUT.json LAND_SYMBOL REQUEST.json\nstory-run EXE OUTPUT.json ADVERTISE.ini SEQUENCE\ncity-rider-pose STAGE_REL_OR_DATA_DLL OUTPUT.json EXE SONICMDL.prs ACTION FRAME\ncity-board-glb EXE OUTPUT.glb VARIANT OBJTEX_STG13.prs\ncity-board-model EXE OUTPUT.sa2mdl VARIANT\ncity-board-pose STAGE_REL_OR_DATA_DLL OUTPUT.json EXE VARIANT ACTION FRAME\ncity-collision STAGE_REL_OR_DATA_DLL OUTPUT.json [UNIFORM_SCALE]\ncity-raycast STAGE_REL_OR_DATA_DLL OUTPUT.json REQUEST.json\ncity-spawn-ground STAGE_REL_OR_DATA_DLL OUTPUT.json EXE MAX_DISTANCE\nstage20-splash stg20D.rel OUTPUT.json QUERY\nstage20-entry stg20D.rel OUTPUT.json REQUEST.json\nstage-fog FOG.bin OUTPUT.json\nstage-light-table STAGE_light.bin OUTPUT.json\nstage-light-base STAGE_light.bin OUTPUT.json\nmessage-bank MESSAGE.prs OUTPUT.json\nstage-registration STAGE.rel OUTPUT.json\ncity-board-resources stg13D.rel OUTPUT.json\nplayer-physics EXE OUTPUT.json\nstage-bind SET OUTPUT.json EXE sa2lvl.ini ObjectLists.ini LEVEL_SECTION\nstage-positions EXE OUTPUT.json STAGE_ID\ndeath-zones EXE OUTPUT.json STAGE.ini\nstage-paths EXE OUTPUT.json STAGE.ini\nstory EXE OUTPUT.json ADVERTISE.ini\nevent-frame EVENT.prs OUTPUT.json FRAME\ndemo-frame RAW_DEMO.bin OUTPUT.json FRAME\ncampaign-frame EXE OUTPUT.json ADVERTISE.ini SEQUENCE ACTION_INDEX FRAME\ncampaign-camera EXE OUTPUT.json ADVERTISE.ini SEQUENCE ACTION_INDEX FRAME CAMERA_SLOT\nevent-camera EVENT.prs OUTPUT.json FRAME MOTION.bin CAMERA_SLOT\nfinalhazard-textures EXE OUTPUT.json modeltex_bosslast2.prs\nfinalhazard-visual EXE OUTPUT.json CLIP_INDEX FRAME\nfinalhazard-weighted-pose EXE OUTPUT.json CLIP_INDEX FRAME\nfinalhazard-cancel-placement Boss_Last2D.rel OUTPUT.json REQUEST.json\nfinalhazard-placement-clock Boss_Last2D.rel OUTPUT.json REQUEST.json\nfinalhazard-request-placement Boss_Last2D.rel OUTPUT.json REQUEST.json\nfinalhazard-retract Boss_Last2D.rel OUTPUT.json REQUEST.json\nfinalhazard-contact Boss_Last2D.rel OUTPUT.json REQUEST.json\nfinalhazard-weakpoint-update Boss_Last2D.rel OUTPUT.json GATE_BYTE MODE\nfinalhazard-pose EXE OUTPUT.json CLIP_INDEX FRAME\nfinalhazard-polygons EXE OUTPUT.json\nfinalhazard-mode5-tail Boss_Last2D.rel OUTPUT.json TIMER SUBMODE WORK24\nfinalhazard-origins Boss_Last2D.rel OUTPUT.json EXE CLIP_INDEX FRAME\nfinalhazard-placements Boss_Last2D.rel OUTPUT.json\nfinalhazard-weights EXE OUTPUT.json\nfinalhazard-motions EXE OUTPUT.json\nfinalhazard-geometry EXE OUTPUT.json\nfinalhazard-rigs EXE OUTPUT.json\nhot-shot-target-display boss_hotshotD.rel OUTPUT.json STATE.json\nhot-shot-target-rotation boss_hotshotD.rel OUTPUT.json STATE.json\nhot-shot-target-clock boss_hotshotD.rel OUTPUT.json STATE.json\nhot-shot-target-follow boss_hotshotD.rel OUTPUT.json STATE.json\nhot-shot-mode-nine boss_hotshotD.rel OUTPUT.json STATE.json\nhot-shot-settle boss_hotshotD.rel OUTPUT.json STATE.json\nhot-shot-mode-two-exit boss_hotshotD.rel OUTPUT.json STATE.json\nhot-shot-mode-two-step boss_hotshotD.rel OUTPUT.json STATE.json\nhot-shot-mode-two-opening boss_hotshotD.rel OUTPUT.json POLLS READY_ON_LAST(true|false)\nhot-shot-intro-clock boss_hotshotD.rel OUTPUT.json UPDATES\nhot-shot-arena boss_hotshotD.rel OUTPUT.json X Y Z\ncity-board-draw-defaults stg13D.rel OUTPUT.json DRAW_INPUT.json";
pub(super) fn arity(cmd: &str) -> Option<usize> {
    match cmd {
        "kart-time-extension"
        | "finalhazard-polygons"
        | "finalhazard-placements"
        | "finalhazard-weights"
        | "finalhazard-motions"
        | "finalhazard-geometry"
        | "finalhazard-rigs"
        | "duel-platform"
        | "story-progress"
        | "mission-ranks"
        | "character-motion-bank"
        | "character-animations"
        | "kart-vehicles"
        | "kart-courses"
        | "player-physics"
        | "stage-fog"
        | "stage-light-table"
        | "stage-light-base"
        | "message-bank"
        | "stage-registration"
        | "city-board-resources" => Some(3),
        "fan-goal" | "fan-replay" | "fan-rank" => Some(9),
        "campaign-camera" => Some(8),
        "finalhazard-mode5-tail"
        | "finalhazard-origins"
        | "kart-story-gate-sequence"
        | "kart-normal-contact"
        | "kart-contact-recovery"
        | "kart-course-recovery"
        | "kart-raycast"
        | "kart-piece-collision"
        | "event-camera"
        | "hot-shot-arena"
        | "fan-stage" => Some(6),
        "fan-emerald-contact"
        | "kart-course-materials"
        | "kart-course-background"
        | "kart-course-billboards"
        | "fan-wild-canyon"
        | "fan-big-foot"
        | "character-state-pose"
        | "sonic-state-pose"
        | "city-rider-pose"
        | "city-board-pose"
        | "stage-bind"
        | "campaign-frame" => Some(7),
        "finalhazard-weakpoint-update"
        | "finalhazard-visual"
        | "finalhazard-weighted-pose"
        | "finalhazard-pose"
        | "kart-story-gates"
        | "kart-native-layout"
        | "kart-balloon-layout"
        | "kart-ring-groups"
        | "kart-buffer-tuning"
        | "kart-finish-display"
        | "demo-prs-window"
        | "stage-result"
        | "damage-ring"
        | "stage-raycast"
        | "story-run"
        | "character-camera"
        | "hot-shot-mode-two-opening"
        | "city-spawn-ground" => Some(5),
        "duel-generator"
        | "duel-platform-update"
        | "duel-meteor"
        | "duel-debris"
        | "duel-dust"
        | "duel-stage-frame"
        | "duel-stage-setup"
        | "duel-fog"
        | "duel-director"
        | "duel-gauge"
        | "duel-title"
        | "duel-display" => Some(4),
        "finalhazard-cancel-placement"
        | "finalhazard-placement-clock"
        | "finalhazard-request-placement"
        | "finalhazard-retract"
        | "finalhazard-contact"
        | "finalhazard-textures"
        | "city-board-draw-defaults"
        | "story-sections"
        | "stage-positions"
        | "death-zones"
        | "stage-paths"
        | "story"
        | "event-frame"
        | "demo-frame"
        | "fan-demos"
        | "fan-boss-rushes"
        | "fan-starts"
        | "fan-levels"
        | "kart-drift-step"
        | "kart-drift-windup"
        | "kart-scripted-motion"
        | "kart-active-profile"
        | "kart-vehicle-init"
        | "kart-spin-drive"
        | "kart-recovery-drive"
        | "kart-race-tuning"
        | "kart-guided-drive"
        | "kart-contact-step"
        | "kart-fall-recovery"
        | "kart-ring-particle"
        | "kart-balloon-display"
        | "kart-dash-display"
        | "kart-ring-display"
        | "kart-ring-pickup"
        | "kart-driving-audio"
        | "kart-update-gate"
        | "kart-drive-input"
        | "kart-timeout"
        | "kart-pursuit-clock"
        | "kart-target-model"
        | "kart-target-textures"
        | "kart-target-display"
        | "kart-story-target"
        | "kart-finish-menu"
        | "kart-field-update"
        | "kart-standings"
        | "kart-crossing-directory"
        | "kart-segment-geometry"
        | "kart-progress-tick"
        | "kart-race-gate-tick"
        | "kart-story-gate-display"
        | "kart-gate-tick"
        | "kart-ramp-response"
        | "kart-dash-panel"
        | "kart-ramp-tick"
        | "kart-ramp-init"
        | "kart-objects"
        | "character-motion"
        | "fan-tasks"
        | "stage20-splash"
        | "hot-shot-target-display"
        | "hot-shot-target-rotation"
        | "hot-shot-target-clock"
        | "hot-shot-target-follow"
        | "hot-shot-mode-nine"
        | "hot-shot-settle"
        | "hot-shot-mode-two-exit"
        | "hot-shot-mode-two-step"
        | "hot-shot-intro-clock"
        | "fan-stage-reuse"
        | "stage20-entry" => Some(4),
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
            } else if sa2_asset_tools::story_movie::supported(id)
                && install
                    .join(format!("resource/gd_PC/event/e{id:04}_m.sfd"))
                    .try_exists()?
            {
                if args.get(7).is_some() {
                    return Err(Error::invalid("movie pictures have no event camera slot"));
                }
                let name = format!("resource/gd_PC/event/e{id:04}_m.sfd");
                let movie = Extra::read(
                    &install.join(&name),
                    name,
                    80 * 1024 * 1024,
                    "story-event-movie",
                )?;
                let i = extras.len();
                extras.push(movie);
                sa2_asset_tools::story_movie::movie_picture(id, &extras[i].bytes, frame)?
            } else {
                json!({"available":false,"missingMainEvent":logical,"reason":"No matching main event PRS; movie or special-event dispatch is not implemented."})
            }
        }
        story::Action::Level { level, character } => {
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
            let rows = stage_positions::for_stage(exe, level as u16)?;
            let (pose, defaults) = story_run::level_player(exe, character, level, &rows)?;
            let mut payload = json!({"entryPose":pose,"playerDefaults":defaults,"entryPoses":rows,"normalCityLayers":layers,
                "scope":"Authored character poses and available layer bindings. No character dynamics, mission execution or stage-clear transition."});
            if level == 60 && character == 0 {
                let i = load(
                    extras,
                    "resource/gd_PC/boss_bigfootD.rel".into(),
                    "boss-stage-program",
                )?;
                let trace=sa2_asset_tools::big_foot::replay(&extras[i].bytes, br#"{"work_allocation_ok":true,"frames":[],"intro":{"updates":1,"helper_task_created":false}}"#)?;
                payload["bossIntroPreview"] = json!({"requiresBossWorkAllocation":true,"stageSetup":trace["boss"],"firstCallback":trace["intro"]["frames"][0],"scope":"First scheduled boss intro update after successful setup; the earlier player spawn and yaw are unknown."});
            }
            if level == 61 && character == 1 {
                let i = load(
                    extras,
                    "resource/gd_PC/boss_hotshotD.rel".into(),
                    "boss-stage-program",
                )?;
                let reg = sa2_asset_tools::city_rel::stage_registration(&extras[i].bytes)?;
                payload["bossIntroPreview"] = json!({"stageSetup":reg["bossTaskSetup"],"introPlayerWrite":reg["introPlayerWrite"],"scope":"Slot-zero position written in intro submode zero. Earlier player spawn, yaw and encounter progression are unknown."});
            }
            payload
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
        "duel-generator" => {
            let path = Path::new(&args[3]);
            let req = Extra::read(
                path,
                basename(path)?.to_owned(),
                16 * 1024 * 1024,
                "generator-frames",
            )?;
            let result = sa2_asset_tools::duel_platform::replay(&bytes, &req.bytes)?;
            extras.push(req);
            result
        }
        "duel-display" => {
            let path = Path::new(&args[3]);
            let req = Extra::read(
                path,
                basename(path)?.to_owned(),
                4 * 1024 * 1024,
                "display-request",
            )?;
            let result = sa2_asset_tools::duel_display::replay(&bytes, &req.bytes)?;
            extras.push(req);
            result
        }
        "duel-title" => {
            let path = Path::new(&args[3]);
            let req = Extra::read(
                path,
                basename(path)?.to_owned(),
                4 * 1024 * 1024,
                "title-ops",
            )?;
            let result = sa2_asset_tools::duel_title::replay(&bytes, &req.bytes)?;
            extras.push(req);
            result
        }
        "duel-gauge" => {
            let path = Path::new(&args[3]);
            let req = Extra::read(
                path,
                basename(path)?.to_owned(),
                1024 * 1024,
                "gauge-update",
            )?;
            let result = sa2_asset_tools::duel_gauge::replay(&bytes, &req.bytes)?;
            extras.push(req);
            result
        }
        "duel-director" => {
            let path = Path::new(&args[3]);
            let req = Extra::read(
                path,
                basename(path)?.to_owned(),
                1024 * 1024,
                "director-frame",
            )?;
            let result = sa2_asset_tools::duel_director::replay(&bytes, &req.bytes)?;
            extras.push(req);
            result
        }
        "duel-fog" => {
            let path = Path::new(&args[3]);
            let req = Extra::read(
                path,
                basename(path)?.to_owned(),
                1024 * 1024,
                "fog-callback",
            )?;
            let result = sa2_asset_tools::duel_fog::replay(&bytes, &req.bytes)?;
            extras.push(req);
            result
        }
        "duel-stage-setup" => {
            let path = Path::new(&args[3]);
            let req = Extra::read(
                path,
                basename(path)?.to_owned(),
                16 * 1024 * 1024,
                "stage-setup",
            )?;
            let result = sa2_asset_tools::duel_stage_setup::replay(&bytes, &req.bytes)?;
            extras.push(req);
            result
        }
        "duel-stage-frame" => {
            let path = Path::new(&args[3]);
            let req = Extra::read(
                path,
                basename(path)?.to_owned(),
                16 * 1024 * 1024,
                "stage-frames",
            )?;
            let result = sa2_asset_tools::duel_stage::replay(&bytes, &req.bytes)?;
            extras.push(req);
            result
        }
        "duel-dust" => {
            let path = Path::new(&args[3]);
            let req = Extra::read(
                path,
                basename(path)?.to_owned(),
                16 * 1024 * 1024,
                "dust-call",
            )?;
            let result = sa2_asset_tools::duel_dust::replay(&bytes, &req.bytes)?;
            extras.push(req);
            result
        }
        "duel-debris" => {
            let path = Path::new(&args[3]);
            let req = Extra::read(
                path,
                basename(path)?.to_owned(),
                16 * 1024 * 1024,
                "debris-ops",
            )?;
            let result = sa2_asset_tools::duel_debris::replay(&bytes, &req.bytes)?;
            extras.push(req);
            result
        }
        "duel-meteor" => {
            let path = Path::new(&args[3]);
            let req = Extra::read(
                path,
                basename(path)?.to_owned(),
                16 * 1024 * 1024,
                "meteor-frames",
            )?;
            let result = sa2_asset_tools::duel_fireball::replay(&bytes, &req.bytes)?;
            extras.push(req);
            result
        }
        "duel-platform-update" => {
            let path = Path::new(&args[3]);
            let req = Extra::read(
                path,
                basename(path)?.to_owned(),
                16 * 1024 * 1024,
                "platform-frames",
            )?;
            let result = sa2_asset_tools::duel_platform_motion::replay(&bytes, &req.bytes)?;
            extras.push(req);
            result
        }
        "mission-ranks" => serde_json::to_value(sa2_asset_tools::mission_ranks::read(&bytes)?)?,
        "fan-emerald-contact"
        | "fan-wild-canyon"
        | "fan-big-foot"
        | "fan-rank"
        | "fan-stage"
        | "fan-demos"
        | "fan-boss-rushes"
        | "fan-starts"
        | "fan-stage-reuse"
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
            } else if command == "fan-starts" {
                serde_json::to_value(catalog.story_starts()?)?
            } else if command == "fan-stage-reuse" {
                catalog.stage_reuse()?
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
        "kart-finish-display" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                source_label(path, root)?,
                4 * 1024 * 1024,
                "kart-result-atlas",
            )?);
            let path = Path::new(&args[4]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                4096,
                "kart-result-display",
            )?);
            sa2_asset_tools::kart_finish_menu::display(
                &bytes,
                &extras[0].bytes,
                serde_json::from_slice(&extras[1].bytes)?,
            )?
        }
        "kart-drift-step" | "kart-drift-windup" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                4096,
                "kart-drift-windup",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_drift::tick(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-scripted-motion" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                4096,
                "kart-scripted-motion",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_scripted_motion::step(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-active-profile" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                64 * 1024,
                "kart-active-profile",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_vehicle_init::profile(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-vehicle-init" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                64 * 1024,
                "kart-vehicle-init",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_vehicle_init::init(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-spin-drive" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                64 * 1024,
                "kart-spin-drive",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_drive_input::spin(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-recovery-drive" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                64 * 1024,
                "kart-recovery-drive",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_drive_input::recovery(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-buffer-tuning" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("buffers/{}", basename(path)?),
                32 * 1024 * 1024,
                "kart-setup-buffer",
            )?);
            sa2_asset_tools::kart_guided_drive::buffer_tuning(
                &bytes,
                &extras[0].bytes,
                number(&args[4])?,
            )?
        }
        cmd @ ("kart-normal-contact" | "kart-contact-recovery" | "kart-course-recovery") => {
            let exe = Path::new(&args[3]);
            let request = Path::new(&args[5]);
            extras.push(Extra::read(
                exe,
                source_label(exe, root)?,
                128 * 1024 * 1024,
                "pc-executable",
            )?);
            extras.push(Extra::read(
                request,
                format!("requests/{}", basename(request)?),
                64 * 1024,
                "kart-recovery-request",
            )?);
            if cmd == "kart-normal-contact" {
                serde_json::to_value(sa2_asset_tools::kart_fall_recovery::normal_contact_tick(
                    &bytes,
                    &extras[0].bytes,
                    text_arg(&args[4])?,
                    serde_json::from_slice(&extras[1].bytes)?,
                )?)?
            } else if cmd == "kart-contact-recovery" {
                serde_json::to_value(sa2_asset_tools::kart_fall_recovery::contact_course_tick(
                    &bytes,
                    &extras[0].bytes,
                    text_arg(&args[4])?,
                    serde_json::from_slice(&extras[1].bytes)?,
                )?)?
            } else {
                serde_json::to_value(sa2_asset_tools::kart_fall_recovery::course_tick(
                    &bytes,
                    &extras[0].bytes,
                    text_arg(&args[4])?,
                    serde_json::from_slice(&extras[1].bytes)?,
                )?)?
            }
        }
        "kart-fall-recovery" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                64 * 1024,
                "kart-fall-recovery",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_fall_recovery::tick(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-ring-particle" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                64 * 1024,
                "kart-ring-particle",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_pickup::particle_tick(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-balloon-display" | "kart-dash-display" => {
            let req = Path::new(&args[3]);
            extras.push(Extra::read(
                req,
                format!("requests/{}", basename(req)?),
                4096,
                if command == "kart-balloon-display" {
                    "kart-balloon-display"
                } else {
                    "kart-dash-display"
                },
            )?);
            let pak = input
                .with_file_name("PRS")
                .join(if command == "kart-balloon-display" {
                    "objtex_common.pak"
                } else {
                    "course.pak"
                });
            extras.push(Extra::read(
                &pak,
                source_label(&pak, root)?,
                16 * 1024 * 1024,
                if command == "kart-balloon-display" {
                    "kart-common-texture-bank"
                } else {
                    "kart-course-texture-bank"
                },
            )?);
            if command == "kart-balloon-display" {
                sa2_asset_tools::kart_balloon::display(
                    &bytes,
                    &extras[1].bytes,
                    serde_json::from_slice(&extras[0].bytes)?,
                )?
            } else {
                sa2_asset_tools::kart_ramp::display(
                    &bytes,
                    &extras[1].bytes,
                    serde_json::from_slice(&extras[0].bytes)?,
                )?
            }
        }
        "kart-ring-display" => {
            let req = Path::new(&args[3]);
            extras.push(Extra::read(
                req,
                format!("requests/{}", basename(req)?),
                4096,
                "kart-ring-display",
            )?);
            let pak = input.with_file_name("PRS").join("objtex_common.pak");
            extras.push(Extra::read(
                &pak,
                source_label(&pak, root)?,
                16 * 1024 * 1024,
                "kart-ring-texture-bank",
            )?);
            sa2_asset_tools::kart_pickup::display(
                &bytes,
                &extras[1].bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?
        }
        "kart-ring-pickup" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                64 * 1024,
                "kart-ring-pickup",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_pickup::tick(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-driving-audio" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                64 * 1024,
                "kart-driving-audio",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_driving_audio::tick(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-update-gate" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                64 * 1024,
                "kart-update-gate",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_drive_input::update_gate(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-race-tuning" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                64 * 1024,
                "kart-race-tuning",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_guided_drive::setup(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-guided-drive" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                64 * 1024,
                "kart-guided-drive",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_guided_drive::tick(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-contact-step" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                64 * 1024,
                "kart-movement-contact",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_contact::step(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-drive-input" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                1024 * 1024,
                "kart-drive-input",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_drive_input::replay(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-timeout" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                4096,
                "kart-timeout",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_timeout::step(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-pursuit-clock" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                4096,
                "kart-pursuit-clock",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_story_target::controller(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-target-display" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                4096,
                "kart-target-display",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_story_target::presentation(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-story-target" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                4096,
                "kart-story-target",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_story_target::step(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-finish-menu" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                4096,
                "kart-finish-menu",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_finish_menu::tick(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-field-update" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                8192,
                "kart-field",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_progress::field_update(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-standings" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                4096,
                "kart-standing-slots",
            )?);
            let slots: Vec<sa2_asset_tools::kart_progress::StandingSlot> =
                serde_json::from_slice(&extras[0].bytes)?;
            serde_json::to_value(sa2_asset_tools::kart_progress::standings(&bytes, &slots)?)?
        }
        "kart-crossing-directory" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                source_label(path, root)?,
                128 * 1024 * 1024,
                "pc-executable",
            )?);
            sa2_asset_tools::kart_progress::crossing_directory(&bytes, &extras[0].bytes)?
        }
        "kart-segment-geometry" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                4096,
                "kart-segment-geometry",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_progress::segment_geometry(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-progress-tick" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                4096,
                "kart-segment-observations",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_progress::update(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-time-extension" => {
            let path = input.with_file_name("PRS").join("course.pak");
            extras.push(Extra::read(
                &path,
                source_label(&path, root)?,
                16 * 1024 * 1024,
                "kart-course-texture-bank",
            )?);
            sa2_asset_tools::kart_gate::time_extension(&bytes, &extras[0].bytes)?
        }
        "kart-story-gate-display" | "kart-gate-tick" | "kart-race-gate-tick" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                4096,
                "kart-gate-observations",
            )?);
            if command == "kart-story-gate-display" {
                let mut report = sa2_asset_tools::kart_gate::presentation(
                    &bytes,
                    serde_json::from_slice(&extras[0].bytes)?,
                )?;
                if report["draw"].is_object() {
                    let path = input.with_file_name("PRS").join("course.pak");
                    extras.push(Extra::read(
                        &path,
                        source_label(&path, root)?,
                        16 * 1024 * 1024,
                        "kart-course-texture-bank",
                    )?);
                    sa2_asset_tools::kart_gate::bind_draw_textures(
                        &bytes,
                        &extras[1].bytes,
                        &mut report,
                    )?;
                }
                report
            } else if command == "kart-race-gate-tick" {
                serde_json::to_value(sa2_asset_tools::kart_gate::racing_tick(
                    &bytes,
                    serde_json::from_slice(&extras[0].bytes)?,
                )?)?
            } else {
                serde_json::to_value(sa2_asset_tools::kart_gate::tick(
                    &bytes,
                    serde_json::from_slice(&extras[0].bytes)?,
                )?)?
            }
        }
        "kart-ramp-response" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                4096,
                "kart-response-observations",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_ramp::response(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-dash-panel" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                1024 * 1024,
                "ramp-contact-observations",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_ramp::dash(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-ramp-tick" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                1024 * 1024,
                "ramp-contact-observations",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_ramp::tick(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-ramp-init" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                4096,
                "task-pose",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_ramp::initialize(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?)?
        }
        "kart-story-gate-sequence"
        | "kart-story-gates"
        | "kart-balloon-layout"
        | "kart-ring-groups" => {
            for (arg, limit, role) in [
                (3, 128 * 1024 * 1024, "pc-executable"),
                (4, 1024 * 1024, "kart-set"),
            ] {
                let path = Path::new(&args[arg]);
                extras.push(Extra::read(path, source_label(path, root)?, limit, role)?);
            }
            if command == "kart-story-gate-sequence" {
                let path = Path::new(&args[5]);
                extras.push(Extra::read(
                    path,
                    format!("requests/{}", basename(path)?),
                    1024 * 1024,
                    "kart-gate-contact-sequence",
                )?);
                sa2_asset_tools::kart_gate::story_sequence(
                    &bytes,
                    &extras[0].bytes,
                    &extras[1].bytes,
                    serde_json::from_slice(&extras[2].bytes)?,
                )?
            } else if command == "kart-story-gates" {
                sa2_asset_tools::kart_gate::story_layout(
                    &bytes,
                    &extras[0].bytes,
                    &extras[1].bytes,
                )?
            } else if command == "kart-balloon-layout" {
                sa2_asset_tools::kart_objects::balloon_layout(
                    &bytes,
                    &extras[0].bytes,
                    &extras[1].bytes,
                )?
            } else {
                sa2_asset_tools::kart_objects::ring_groups(
                    &bytes,
                    &extras[0].bytes,
                    &extras[1].bytes,
                )?
            }
        }
        "kart-objects" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                source_label(path, root)?,
                128 * 1024 * 1024,
                "pc-executable",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_objects::read(
                &extras[0].bytes,
                &bytes,
            )?)?
        }
        "kart-vehicles" => {
            serde_json::to_value(sa2_asset_tools::fan_stage::kart_profiles(&bytes)?)?
        }
        "kart-courses" => serde_json::to_value(sa2_asset_tools::kart_course::courses(&bytes)?)?,
        "kart-raycast" => {
            let dll = Path::new(&args[3]);
            extras.push(Extra::read(
                dll,
                source_label(dll, root)?,
                128 * 1024 * 1024,
                "kart-course-models",
            )?);
            let p = Path::new(&args[5]);
            extras.push(Extra::read(
                p,
                format!("requests/{}", basename(p)?),
                64 * 1024,
                "ray-request",
            )?);
            let req: RayRequest = serde_json::from_slice(&extras[1].bytes)?;
            if req.uniform_scale != 1.0 {
                return Err(Error::invalid("kart ray requires native units"));
            }
            let route = text_arg(&args[4])?;
            let query = sa2_asset_tools::kart_course::course_raycast(
                &bytes,
                &extras[0].bytes,
                route,
                req.origin,
                req.direction,
                req.max_distance,
            )?;
            let route = route
                .parse::<u16>()
                .map_or_else(|_| json!(route), |n| json!(n));
            json!({"route":route,"request":req,"hit":query.hit,"occurrencesWithoutCollision":query.without_collision,"scope":"static course geometry in native units"})
        }
        "kart-piece-collision" => {
            for (i, kind, limit) in [
                (3, "kart-course-models", 128 * 1024 * 1024),
                (5, "activation-cases", 1024 * 1024),
            ] {
                let path = Path::new(&args[i]);
                extras.push(Extra::read(path, source_label(path, root)?, limit, kind)?);
            }
            let cases: Vec<serde_json::Value> = serde_json::from_slice(&extras[1].bytes)?;
            let invalid =
                || Error::invalid("expected occurrence, count, raceMode and six kartOwners");
            let activation = cases
                .iter()
                .map(|c| {
                    let n = |k: &str| c[k].as_i64().ok_or_else(invalid);
                    let owners = c["kartOwners"]
                        .as_array()
                        .filter(|a| a.len() == 6)
                        .ok_or_else(invalid)?;
                    let mut slots = [None; 6];
                    for (slot, v) in slots.iter_mut().zip(owners) {
                        *slot = if v.is_null() {
                            None
                        } else {
                            Some(v.as_i64().ok_or_else(invalid)? as i32)
                        };
                    }
                    Ok(sa2_asset_tools::kart_course::piece_collision_active(
                        u16::try_from(n("occurrence")?).map_err(|_| invalid())?,
                        n("count")? as i32,
                        n("raceMode")? as i32,
                        &slots,
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            json!({"pieces": sa2_asset_tools::kart_course::piece_collision_tasks(
                &bytes,
                &extras[0].bytes,
                text_arg(&args[4])?,
            )?, "activation": activation})
        }
        "kart-native-layout" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                source_label(path, root)?,
                128 * 1024 * 1024,
                "kart-course-models",
            )?);
            serde_json::to_value(sa2_asset_tools::kart_course::native_course_placements(
                &bytes,
                &extras[0].bytes,
                text_arg(&args[4])?,
            )?)?
        }
        "kart-course-billboards" => {
            for (i, kind, limit) in [
                (3, "kart-course-models", 128 * 1024 * 1024),
                (4, "kart-module", 16 * 1024 * 1024),
                (6, "rand-observations", 4 * 1024 * 1024),
            ] {
                let path = Path::new(&args[i]);
                extras.push(Extra::read(path, source_label(path, root)?, limit, kind)?);
            }
            let rand: Vec<i32> = serde_json::from_slice(&extras[2].bytes)?;
            serde_json::to_value(sa2_asset_tools::kart_course::course_billboards(
                &bytes,
                &extras[0].bytes,
                &extras[1].bytes,
                text_arg(&args[5])?,
                &rand,
            )?)?
        }
        "kart-course-background" => {
            for (i, kind, limit) in [
                (3, "kart-course-models", 128 * 1024 * 1024),
                (4, "kart-module", 16 * 1024 * 1024),
                (6, "frame-list", 1024 * 1024),
            ] {
                let path = Path::new(&args[i]);
                extras.push(Extra::read(path, source_label(path, root)?, limit, kind)?);
            }
            let frames: Vec<u32> = serde_json::from_slice(&extras[2].bytes)?;
            let sea = frames
                .iter()
                .map(|&frame| {
                    Ok(json!({"frame": frame, "colors": sa2_asset_tools::kart_course::sea_colors(&extras[1].bytes, frame)?}))
                })
                .collect::<Result<Vec<_>>>()?;
            json!({"background": sa2_asset_tools::kart_course::course_background(
                &bytes,
                &extras[0].bytes,
                &extras[1].bytes,
                text_arg(&args[5])?,
            )?, "sea": sea})
        }
        "kart-course-materials" => {
            for (i, kind, limit) in [
                (3, "kart-course-models", 128),
                (4, "kart-module", 16),
                (5, "kart-course-textures", 64),
            ] {
                let path = Path::new(&args[i]);
                extras.push(Extra::read(
                    path,
                    source_label(path, root)?,
                    limit * 1024 * 1024,
                    kind,
                )?);
            }
            serde_json::to_value(sa2_asset_tools::kart_course::course_materials(
                &bytes,
                &extras[0].bytes,
                &extras[1].bytes,
                &extras[2].bytes,
                text_arg(&args[6])?,
            )?)?
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
        "character-state-pose" | "sonic-state-pose" | "city-rider-pose" => {
            for (arg, role) in [
                (3, "pc-executable"),
                (
                    4,
                    if command == "character-state-pose" {
                        "character-model"
                    } else {
                        "sonic-rider-model"
                    },
                ),
            ] {
                let path = Path::new(&args[arg]);
                extras.push(Extra::read(
                    path,
                    source_label(path, root)?,
                    128 * 1024 * 1024,
                    role,
                )?);
            }
            let frame = finite_argument(&args[6])?;
            if command != "city-rider-pose" {
                let sample = if command == "sonic-state-pose" {
                    sa2_asset_tools::character_motions::sonic_pose
                } else {
                    sa2_asset_tools::character_motions::character_pose
                };
                sample(
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
        "kart-target-model" => {
            sa2_asset_tools::kart_story_target::model(&bytes, number(&args[3])?)?
        }
        "kart-target-textures" => {
            let (name, count, expected_hash) =
                sa2_asset_tools::kart_story_target::texture_resource(&bytes, number(&args[3])?)?;
            let path = input.with_file_name(&name);
            let archive = Extra::read(
                &path,
                source_label(&path, root)?,
                16 * 1024 * 1024,
                "kart-pursuit-textures",
            )?;
            if sa2_asset_tools::manifest::sha256(&archive.bytes) != expected_hash {
                return Err(Error::invalid(
                    "unexpected kart pursuit texture archive revision",
                ));
            }
            let report = sa2_asset_tools::archive::texture_bank_report(&archive.bytes, count)?;
            extras.push(archive);
            report
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
            let report = sa2_asset_tools::archive::texture_bank_report(&archive.bytes, count)?;
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
        "hot-shot-target-display"
        | "hot-shot-target-rotation"
        | "hot-shot-target-clock"
        | "hot-shot-target-follow"
        | "hot-shot-mode-nine"
        | "hot-shot-settle"
        | "hot-shot-mode-two-step"
        | "hot-shot-mode-two-exit" => {
            let path = Path::new(&args[3]);
            let req = Extra::read(path, basename(path)?.to_owned(), 65536, "boss-state")?;
            let step = if command == "hot-shot-target-display" {
                {
                    let mut draw =
                        sa2_asset_tools::city_rel::hot_shot_target_display(&bytes, &req.bytes)?;
                    if let Some(names) = draw["textureNames"].as_array() {
                        let path = input.with_file_name("grobo_eff2_tex.prs");
                        let tex = Extra::read(
                            &path,
                            source_label(&path, root)?,
                            16 * 1024 * 1024,
                            "target-texture-archive",
                        )?;
                        let decoded =
                            sa2_asset_tools::archive::decompress_prs(&tex.bytes, 32 * 1024 * 1024)?;
                        let entries = sa2_asset_tools::archive::read_gvm(&decoded)?;
                        if names.len() != entries.len()
                            || !names
                                .iter()
                                .zip(&entries)
                                .all(|(n, e)| n.as_str() == Some(e.name.as_str()))
                        {
                            return Err(Error::invalid(
                                "target texture list does not match installed GVM",
                            ));
                        }
                        draw["textureEntries"] = json!(entries);
                        extras.push(tex);
                    }
                    draw
                }
            } else if command == "hot-shot-target-rotation" {
                sa2_asset_tools::city_rel::hot_shot_target_rotation(&bytes, &req.bytes)?
            } else if command == "hot-shot-target-clock" {
                sa2_asset_tools::city_rel::hot_shot_target_clock(&bytes, &req.bytes)?
            } else if command == "hot-shot-target-follow" {
                sa2_asset_tools::city_rel::hot_shot_target_follow(&bytes, &req.bytes)?
            } else if command == "hot-shot-mode-nine" {
                sa2_asset_tools::city_rel::hot_shot_mode_nine(&bytes, &req.bytes)?
            } else if command == "hot-shot-settle" {
                sa2_asset_tools::city_rel::hot_shot_settle(&bytes, &req.bytes)?
            } else if command == "hot-shot-mode-two-exit" {
                sa2_asset_tools::city_rel::hot_shot_mode_two_exit(&bytes, &req.bytes)?
            } else {
                sa2_asset_tools::city_rel::hot_shot_mode_two_step(&bytes, &req.bytes)?
            };
            extras.push(req);
            step
        }
        "hot-shot-mode-two-opening" => sa2_asset_tools::city_rel::hot_shot_mode_two_opening(
            &bytes,
            number(&args[3])?,
            number(&args[4])?,
        )?,
        "hot-shot-intro-clock" => {
            sa2_asset_tools::city_rel::hot_shot_intro_clock(&bytes, number(&args[3])?)?
        }
        "hot-shot-arena" => {
            let input = [number(&args[3])?, number(&args[4])?, number(&args[5])?];
            json!({"slot":0,"position":sa2_asset_tools::city_rel::hot_shot_arena(&bytes,input)?})
        }
        "stage-registration" => sa2_asset_tools::city_rel::stage_registration(&bytes)?,
        "city-board-draw-defaults" => {
            use sa2_asset_tools::{
                board_draw::{self, DrawInput, MotionRow},
                city_motion_registry::{CityDefaults, MotionAddress, RegistryCall, Slot},
            };
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                65536,
                "board-draw-input",
            )?);
            let mut input: DrawInput = serde_json::from_slice(&extras[0].bytes)?;
            if !input.motions.is_empty() {
                return Err(Error::invalid(
                    "stage-default draw requires no explicit motion rows",
                ));
            }
            let mut slots = [Slot::default(); 300];
            CityDefaults::read(&bytes)?.apply_call(RegistryCall::PrologRegister, &mut slots);
            for (index, slot) in slots.iter().enumerate() {
                let pointer = match slot.address {
                    None => 0,
                    Some(MotionAddress::Stage {
                        module: 12,
                        section: 5,
                        offset,
                    }) => offset,
                    _ => return Err(Error::invalid("unexpected stage-default motion address")),
                };
                input.motions.push(MotionRow {
                    index: index as u16,
                    pointer,
                });
            }
            json!({"motionAddressSpace":"REL module 12 section 5 offsets; zero is null",
                "registry":"empty before stage entry; no live overrides",
                "plan":board_draw::plan(&input).map_err(Error::invalid)?})
        }
        "finalhazard-textures" => {
            let archive = Path::new(&args[3]);
            extras.push(Extra::read(
                archive,
                source_label(archive, root)?,
                32 * 1024 * 1024,
                "original-model-textures",
            )?);
            sa2_asset_tools::finalhazard::textures(&bytes, &extras[0].bytes)?
        }
        "finalhazard-visual" => serde_json::to_value(sa2_asset_tools::finalhazard_visual::sample(
            &bytes,
            number(&args[3])?,
            finite_argument(&args[4])?,
        )?)?,
        "finalhazard-weighted-pose" => sa2_asset_tools::finalhazard::weighted_pose(
            &bytes,
            number(&args[3])?,
            finite_argument(&args[4])?,
        )?,
        "finalhazard-cancel-placement" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                4096,
                "cancel-placement",
            )?);
            sa2_asset_tools::finalhazard::cancel_placement(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?
        }
        "finalhazard-placement-clock" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                // Handoff replays carry one object per frame, with optional
                // stage and debris blocks.
                1 << 20,
                "placement-clock",
            )?);
            sa2_asset_tools::finalhazard::placement_clock(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?
        }
        "finalhazard-request-placement" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                4096,
                "placement-request",
            )?);
            sa2_asset_tools::finalhazard::request_placement(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?
        }
        "finalhazard-retract" => {
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                4096,
                "weakpoint-state",
            )?);
            sa2_asset_tools::finalhazard::weakpoint_retract(
                &bytes,
                serde_json::from_slice(&extras[0].bytes)?,
            )?
        }
        "finalhazard-contact" => {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Request {
                state: sa2_asset_tools::finalhazard::WeakpointContactState,
                hit: Option<(i32, i32)>,
            }
            let path = Path::new(&args[3]);
            extras.push(Extra::read(
                path,
                format!("requests/{}", basename(path)?),
                4096,
                "contact-observation",
            )?);
            let mut req: Request = serde_json::from_slice(&extras[0].bytes)?;
            let _ = sa2_asset_tools::finalhazard::weakpoint_update_plan(&bytes, 0, req.state.mode)?;
            let accepted = req.state.contact(req.hit);
            json!({"accepted":accepted,"state":req.state,"scope":"Contact handler task-work writes; external calls and boss health not simulated."})
        }
        "finalhazard-weakpoint-update" => sa2_asset_tools::finalhazard::weakpoint_update_plan(
            &bytes,
            number(&args[3])?,
            number(&args[4])?,
        )?,
        "finalhazard-pose" => serde_json::to_value(sa2_asset_tools::finalhazard::pose(
            &bytes,
            number(&args[3])?,
            finite_argument(&args[4])?,
        )?)?,
        "finalhazard-polygons" => sa2_asset_tools::finalhazard::polygon_schedule(&bytes)?,
        "finalhazard-mode5-tail" => sa2_asset_tools::finalhazard::weakpoint_mode5_tail(
            &bytes,
            number(&args[3])?,
            number(&args[4])?,
            finite_argument(&args[5])? as f32,
        )?,
        "finalhazard-origins" => {
            let exe = Path::new(&args[3]);
            extras.push(Extra::read(
                exe,
                source_label(exe, root)?,
                128 * 1024 * 1024,
                "boss-rig-and-motion",
            )?);
            sa2_asset_tools::finalhazard::weakpoint_origins(
                &bytes,
                &extras[0].bytes,
                number(&args[4])?,
                finite_argument(&args[5])?,
            )?
        }
        "finalhazard-placements" => sa2_asset_tools::finalhazard::weakpoint_placements(&bytes)?,
        "finalhazard-weights" => sa2_asset_tools::finalhazard::weights(&bytes)?,
        "finalhazard-motions" => sa2_asset_tools::finalhazard::motions(&bytes)?,
        "finalhazard-geometry" => {
            json!({"attachmentsByRva":sa2_asset_tools::finalhazard::geometry(&bytes)?,"offsetBasis":"nested offsets are executable file offsets"})
        }
        "finalhazard-rigs" => {
            let [boss, weakpoint] = sa2_asset_tools::finalhazard::read(&bytes)?;
            json!({"boss":boss,"weakpoint":weakpoint,"space":"native model-local; addresses are executable RVAs"})
        }
        "city-board-resources" => {
            let rel = sa2_asset_tools::city_rel::CityRel::read(&bytes)?;
            json!({"dataSection":5,"resources":rel.board_resources()?})
        }
        "character-motion-bank" => {
            serde_json::to_value(sa2_asset_tools::character_motions::Bank::read(&bytes)?.rows)?
        }
        "character-camera" => {
            let slot: u16 = number(&args[3])?;
            let frame: f32 = number(&args[4])?;
            let bank = sa2_asset_tools::character_motions::Bank::read(&bytes)?;
            let channels = bank.camera(slot, frame)?;
            json!({"slot":slot,"frame":frame,"channels":channels})
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
