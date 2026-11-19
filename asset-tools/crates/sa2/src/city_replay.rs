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
    for frame in bounded(request.frames)? {
        if frame.range_out
            || frame
                .player_items
                .iter()
                .flatten()
                .any(|item| item & 2 != 0)
        {
            return Err(Error::invalid(
                "fixed ring requires an in-range task and no player magnetic shield",
            ));
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
        let events = ring.step(hit, frame.spin_enabled);
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
        if events.register_collision {
            calls.push(json!(["CCL_Entry"]));
        }
        if events.dead_out {
            calls.push(json!(["DeadOut"]));
        }
        frames.push(json!({"mode":format!("{:?}",ring.mode),"yaw":ring.yaw,"calls":calls}));
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
        (59, "BOARDCOL", 0x5ea210) => Some("ObjectBoardCol"),
        (81, "ADXCHG", 0x5e2d70) => Some("ObjectCeAdxChg"),
        _ => None,
    }
}
/// The caller verifies the executable and resolves the entry in the stage 13 list.
/// Names include native padding. Only explicitly mapped callbacks are supported.
pub fn replay(placement: &Placement, def: &ObjectDefinition, request: &[u8]) -> Result<Value> {
    if !placement
        .position
        .iter()
        .chain(&placement.parameters)
        .all(|v| v.is_finite())
    {
        return Err(Error::invalid("nonfinite placement"));
    }
    match callback_name(placement, def) {
        Some("Ring") => ring(placement, request),
        Some(name @ ("ObjectSpring" | "ObjectSpringB")) => {
            let kind = if placement.object_id == 3 {
                SpringKind::A
            } else {
                SpringKind::B
            };
            let mut value =
                serde_json::to_value(stage_spring_host::replay(placement, kind, request)?)?;
            value["callback"] = json!(name);
            Ok(value)
        }
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
