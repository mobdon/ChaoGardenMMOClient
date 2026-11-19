//! Replay controller for verified City Escape SET entries and explicit engine observations.
use crate::{
    board::{BoardPlayer, BoardTrigger},
    city_music::{self, AdxTrigger},
    object_table::ObjectDefinition,
    set::Placement,
    spring::SpringKind,
    stage_gameplay::{RingHit, RingMode, StageRing},
    stage_spring_host, Error, Result,
};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};

const MAX_BYTES: usize = 1_048_576;
const MAX_FRAMES: usize = 2048;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Frames<F> {
    frames: Vec<F>,
}

fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::invalid("city replay exceeds 1 MiB"));
    }
    Ok(serde_json::from_slice(bytes)?)
}

fn bounded<F>(frames: Vec<F>) -> Result<Vec<F>> {
    if !(1..=MAX_FRAMES).contains(&frames.len()) {
        return Err(Error::invalid("city replay requires 1 to 2048 frames"));
    }
    Ok(frames)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RingFrame {
    #[serde(default)]
    magnet_players: [Option<crate::ring_magnet::Player>; 2],
    advance_result: Option<[f32; 3]>,
    range_out: bool,
    spin_enabled: bool,
    player_items: [Option<u16>; 2],
    ccl_hit_slot: Option<u8>,
}

fn ring(p: &Placement, request: &[u8]) -> Result<Value> {
    if p.parameters[0] == -1.0 {
        return Err(Error::invalid("terrain-following ring path is unsupported"));
    }
    let request = decode::<Frames<RingFrame>>(request)?;
    let mut ring = StageRing::new(p.position, i32::from(p.rotation[1] as i16));
    let mut frames = Vec::new();
    let mut magnet_timer = 0u16;
    let mut magnet_angle = 0f32;
    for frame in bounded(request.frames)? {
        if frame.range_out {
            return Err(Error::invalid("fixed ring requires an in-range task"));
        }
        let magnetic = if ring.mode == RingMode::Active {
            crate::ring_magnet::select(
                ring.position,
                magnet_timer,
                frame.player_items,
                frame.magnet_players,
            )?
        } else {
            None
        };
        let advance = magnetic.as_ref().and_then(|m| m.advance.as_ref());
        match (advance, frame.advance_result) {
            (Some(_), Some(p)) if p.iter().all(|n| n.is_finite()) => {}
            (None, None) => {}
            _ => {
                return Err(Error::invalid(
                    "expected finite advance_result exactly when CalcAdvanceAsPossible is called",
                ))
            }
        }
        if frame.ccl_hit_slot.is_some() && ring.mode != RingMode::Active {
            return Err(Error::invalid("ring contact requires an active task"));
        }
        let hit = match frame.ccl_hit_slot {
            Some(slot @ 0..=1) => Some(RingHit {
                player: slot,
                item: frame.player_items[slot as usize]
                    .ok_or_else(|| Error::invalid("ring hit slot has no player"))?,
            }),
            Some(2) => Some(RingHit { player: 2, item: 0 }),
            Some(_) => {
                return Err(Error::invalid(
                    "ring contact must identify slot 0, 1, or other task 2",
                ))
            }
            None => None,
        };
        let events = ring.step(hit, frame.spin_enabled && magnetic.is_none());
        let mut calls = Vec::new();
        if let Some(slot) = events.player_reward {
            calls.extend([
                json!(["AddScore", 10]),
                json!(["AddNumRing", slot, 1]),
                json!(["AddMechHP", slot, 0.1]),
                json!(["SE_CallRing", slot]),
            ]);
        }
        if events.particles {
            calls.push(json!(["o_ring_0"]));
        }
        if let Some(a) = advance {
            calls.push(json!([
                "CalcAdvanceAsPossible",
                ring.position,
                a.center,
                a.step
            ]));
            ring.position = frame.advance_result.unwrap();
            magnet_timer = magnet_timer.wrapping_add(1);
        }
        if magnetic.is_some() {
            magnet_angle += 3.;
        }
        if events.register_collision {
            calls.push(json!(["CCL_Entry"]));
        }
        if events.dead_out {
            calls.push(json!(["DeadOut"]));
        }
        frames.push(json!({"mode":format!("{:?}",ring.mode),"yaw":ring.yaw,"position":ring.position,"magneticSlot":magnetic.map(|m|m.slot),"magneticTimer":magnet_timer,"magneticAngle":magnet_angle,"calls":calls}));
    }
    Ok(json!({"callback":"Ring","frames":frames}))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Player {
    position: [f32; 3],
    mode: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BoardFrame {
    range_out: bool,
    players: [Option<Player>; 2],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BoardRequest {
    two_player: bool,
    frames: Vec<BoardFrame>,
}

fn board(p: &Placement, request: &[u8]) -> Result<Value> {
    let request = decode::<BoardRequest>(request)?;
    let mut task = BoardTrigger::new(p.position, p.parameters[0], p.rotation[0]);
    let mut frames = Vec::new();
    for frame in bounded(request.frames)? {
        if frame
            .players
            .iter()
            .flatten()
            .any(|p| !p.position.iter().all(|x| x.is_finite()))
        {
            return Err(Error::invalid("nonfinite board player position"));
        }
        let players = frame.players.map(|p| {
            p.map(|p| BoardPlayer {
                position: p.position,
                mode: p.mode,
            })
        });
        let calls: Vec<_> = task
            .tick(players, request.two_player, frame.range_out)
            .into_iter()
            .map(|(slot, code)| json!(["SetInputP", slot, code, 0]))
            .collect();
        frames.push(json!({"calls":calls}));
    }
    Ok(json!({"callback":"ObjectBoardCol","frames":frames}))
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum AdxFrame {
    Bind {
        range_out: bool,
        short_city_2p: bool,
    },
    Tick {
        range_out: bool,
        player_in_sphere: bool,
        game_state: i32,
    },
    Destroy {
        game_state: i32,
        predicate_8001cc18: bool,
    },
}

fn adx(request: &[u8]) -> Result<Value> {
    let request = decode::<Frames<AdxFrame>>(request)?;
    let mut task = None;
    let mut destroyed = false;
    let mut frames = Vec::new();
    for frame in bounded(request.frames)? {
        let calls = match frame {
            AdxFrame::Bind {
                range_out,
                short_city_2p,
            } if task.is_none() && !destroyed => {
                // The engine stage predicate is independent of the two-player SET filename.
                task = AdxTrigger::spawn(range_out, short_city_2p);
                Vec::new()
            }
            AdxFrame::Tick {
                range_out,
                player_in_sphere,
                game_state,
            } => task
                .as_mut()
                .ok_or_else(|| Error::invalid("ADX tick requires a bound task"))?
                .tick(range_out, player_in_sphere, game_state),
            AdxFrame::Destroy {
                game_state,
                predicate_8001cc18,
            } => {
                let task = task
                    .take()
                    .ok_or_else(|| Error::invalid("ADX destroy requires a bound task"))?;
                destroyed = true;
                task.destroy(game_state, predicate_8001cc18)
            }
            _ => return Err(Error::invalid("ADX task already bound or destroyed")),
        };
        frames
            .push(json!({"mode":task.as_ref().map(|t|t.mode),"destroyed":destroyed,"calls":calls}));
    }
    Ok(json!({"callback":"ObjectCeAdxChg","frames":frames}))
}

pub fn callback_name(p: &Placement, d: &ObjectDefinition) -> Option<&'static str> {
    match (p.object_id, d.name.as_str(), d.function_va) {
        (0, "RING   ", 0x6c0f80) => Some("Ring"),
        (3, "SPRA   ", 0x6c4480) => Some("ObjectSpring"),
        (4, "SPRB   ", 0x6c4e90) => Some("ObjectSpringB"),
        (8, "SAVEPOINT", 0x6d99a0) => Some("ObjectSavePoint"),
        (59, "BOARDCOL", 0x5ea210) => Some("ObjectBoardCol"),
        (77, "BOARD", 0x5ebc50) => Some("ObjectBoard"),
        (81, "ADXCHG", 0x5e2d70) => Some("ObjectCeAdxChg"),
        _ => None,
    }
}
/// Shared task identity comes from the verified stage table, not a universal SET index.
pub fn mounted_callback(level: u16, p: &Placement, d: &ObjectDefinition) -> Option<&'static str> {
    match (d.function_va, d.name.as_str()) {
        (0x6c0f80, "RING" | "RING   ") => (p.parameters[0] != -1.).then_some("Ring"),
        (0x6c4480, "SPRA   ") => Some("ObjectSpring"),
        (0x6c4e90, "SPRB   ") => Some("ObjectSpringB"),
        (0x6d95e0, "CHAOPIPE" | "chaopipe") => Some("ObjectChaoPipe"),
        (0x6de3b0, "CONTCHAO" | "contchao") => Some("ContainerChao"),
        // Installed modules with verified checkpoint gate and handoff spans.
        (0x6d99a0, "SAVEPOINT")
            if level != 13
                && matches!(level,0|3..=18|21..=28|30..=32|34..=38|40..=41|43..=44|46..=55|57..=59) =>
        {
            Some("CheckpointContact")
        }
        _ if level == 13 => callback_name(p, d),
        _ => None,
    }
}
pub(crate) fn mounted_replay(
    level: u16,
    p: &Placement,
    d: &ObjectDefinition,
    request: &[u8],
) -> Result<Value> {
    let name = mounted_callback(level, p, d);
    if name == Some("CheckpointContact") {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ContactRequest {
            checkpoint_byte: u8,
            hit: Option<(i32, u32)>,
            players: [crate::checkpoint::ContactPlayer; 2],
        }
        let r: ContactRequest = decode(request)?;
        let mut state = crate::checkpoint::ContactState {
            mode: 1,
            slot_timers: [0; 2],
        };
        let handoff = state.accept(r.hit, &r.players, r.checkpoint_byte)?;
        return Ok(
            json!({"callback":"CheckpointContact","mode":state.mode,"slotTimers":state.slot_timers,"handoff":handoff,"scope":"One idle contact check; checkpoint byte and hit are observed."}),
        );
    }
    if name == Some("ContainerChao") {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ContainerRequest {
            stage_flags: u32,
            frames: Vec<crate::stage_container::Frame>,
            key: Option<crate::stage_key::Request>,
        }
        let req = decode::<ContainerRequest>(request)?;
        return crate::stage_container::replay(p, req.stage_flags, bounded(req.frames)?, req.key);
    }
    if name == Some("ObjectChaoPipe") {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct PipeRequest {
            slot_zero_character: i8,
            frames: Vec<crate::stage_pipe::Frame>,
        }
        let req = decode::<PipeRequest>(request)?;
        return crate::stage_pipe::replay(p, req.slot_zero_character, bounded(req.frames)?);
    }
    replay_named(name, p, request)
}
/// The caller verifies the executable and resolves the entry in the stage 13 list.
/// Names include native padding. Only explicitly mapped callbacks are supported.
pub fn replay(placement: &Placement, def: &ObjectDefinition, request: &[u8]) -> Result<Value> {
    replay_named(callback_name(placement, def), placement, request)
}
fn replay_named(name: Option<&str>, placement: &Placement, request: &[u8]) -> Result<Value> {
    if !placement
        .position
        .iter()
        .chain(&placement.parameters)
        .all(|v| v.is_finite())
    {
        return Err(Error::invalid("nonfinite placement"));
    }
    match name {
        Some("Ring") => ring(placement, request),
        Some(name @ ("ObjectSpring" | "ObjectSpringB")) => {
            let kind = if name == "ObjectSpring" {
                SpringKind::A
            } else {
                SpringKind::B
            };
            let mut value =
                serde_json::to_value(stage_spring_host::replay(placement, kind, request)?)?;
            value["callback"] = json!(name);
            Ok(value)
        }
        Some("ObjectBoard") => crate::board_replay::replay(
            placement,
            bounded(decode::<Frames<crate::board_replay::Step>>(request)?.frames)?,
        ),
        Some("ObjectSavePoint") => checkpoint(placement, request),
        Some("ObjectBoardCol") => board(placement, request),
        Some("ObjectCeAdxChg") => adx(request),
        _ => Err(Error::invalid(
            "unsupported City Escape object ID, padded name, or PC function VA",
        )),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BgmRequest {
    two_player: bool,
    stage_variant: i32,
    frames: Vec<BgmFrame>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BgmFrame {
    game_state: i32,
    player_modes: [u8; 2],
}

pub fn replay_bgm(request: &[u8]) -> Result<Value> {
    let req = decode::<BgmRequest>(request)?;
    if req.frames.len() > MAX_FRAMES {
        return Err(Error::invalid("too many BgmStart frames"));
    }
    let (mut task, prolog) = city_music::prolog(req.stage_variant, req.two_player);
    let mut frames = Vec::new();
    for frame in req.frames {
        let bgm = task
            .as_mut()
            .ok_or_else(|| Error::invalid("stage prolog did not schedule BgmStart"))?;
        let calls = bgm.tick(frame.game_state, frame.player_modes);
        frames.push(json!({"counter":bgm.counter,"calls":calls}));
    }
    Ok(json!({"callback":"CityEscapeProlog/BgmStart","prolog":prolog,"frames":frames}))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckpointReplay {
    engine_result: u32,
    stage_byte: i8,
    frames: Vec<CheckpointFrame>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckpointFrame {
    hit: Option<(i32, u32)>,
    players: [crate::checkpoint::ContactPlayer; 2],
    contact_speed: f32,
}

fn checkpoint(p: &Placement, request: &[u8]) -> Result<Value> {
    use crate::checkpoint::{CheckpointPlacement, CheckpointSpin, CheckpointTicks, ContactState};
    let req: CheckpointReplay = decode(request)?;
    let placement = CheckpointPlacement {
        rotation: p.rotation.map(i32::from),
    };
    let byte = placement.work_byte(req.engine_result, req.stage_byte);
    let mut state = CheckpointTicks {
        contact: ContactState {
            mode: 1,
            slot_timers: [0; 2],
        },
        spin: CheckpointSpin {
            angles: [0x4000, -0x4000],
            steps: [0; 2],
            counters: [0; 2],
        },
    };
    let mut frames = Vec::new();
    for f in bounded(req.frames)? {
        let call = state.tick(f.hit, &f.players, byte, f.contact_speed)?;
        frames.push(
            json!({"mode":state.contact.mode,"slotTimers":state.contact.slot_timers,
            "angles":state.spin.angles,"steps":state.spin.steps,"handoff":call}),
        );
    }
    Ok(
        json!({"callback":"ObjectSavePoint","scope":"Initialized idle checkpoint; hit and signed speed are observations. Handoff is reported, not applied.","frames":frames}),
    )
}
