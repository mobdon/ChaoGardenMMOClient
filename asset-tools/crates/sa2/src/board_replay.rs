//! Trace adapter for the physical board. Engine and render observations are explicit.
use crate::{
    board_draw::{self, DrawInput, MotionRow, Player},
    physical_board::{BoardRider, PhysicalBoard},
    set::Placement,
    Error, Result,
};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Step {
    Bind {
        allocation_ok: bool,
    },
    Tick {
        players: [Option<BoardRider>; 2],
    },
    Draw {
        two_player: bool,
        players: [Option<Player>; 2],
        motions: Vec<MotionRow>,
    },
    Destroy,
}

pub(crate) fn replay(p: &Placement, steps: Vec<Step>) -> Result<Value> {
    let mut board = None;
    let mut output = Vec::new();
    let mut suspended = false;
    for (i, step) in steps.into_iter().enumerate() {
        let value = match step {
            Step::Bind { allocation_ok } if i == 0 => {
                board = PhysicalBoard::bind(p.rotation, p.position, allocation_ok);
                json!({"state":board,"calls": if allocation_ok {
                    json!([["syCalloc",1,88],["fn_13_189EC",64]])
                } else { json!([["syCalloc",1,88]]) }})
            }
            Step::Tick { players } if !suspended => {
                let b = board
                    .as_mut()
                    .ok_or_else(|| Error::invalid("board tick requires successful bind"))?;
                let rider = players[b.slot as usize];
                if rider.is_some_and(|r| {
                    !r.frame.is_finite()
                        || !r.position.iter().chain(&r.speed).all(|v| v.is_finite())
                }) {
                    return Err(Error::invalid("nonfinite rider observation"));
                }
                suspended = b.tick(rider);
                if suspended {
                    json!({"stateBeforeExternalCalls":b,"calls":[["fn_800DE3F8"],["CheckRangeOut"]],"requiresExternalUpdate":true})
                } else {
                    json!({"state":b,"calls":[]})
                }
            }
            Step::Draw {
                two_player,
                mut players,
                motions,
            } if !suspended => {
                let b = board
                    .as_ref()
                    .ok_or_else(|| Error::invalid("board draw requires successful bind"))?;
                let plan = board_draw::plan(&DrawInput {
                    two_player,
                    player: players[b.slot as usize].take(),
                    mode: b.mode,
                    position: b.position,
                    angles: b.angles,
                    cached_action: b.cached_action,
                    cached_frame: b.cached_frame,
                    motions,
                })
                .map_err(Error::invalid)?;
                json!({"plan":plan})
            }
            Step::Destroy if board.take().is_some() => {
                json!({"state":null,"calls":[["fn_13_18AA8"],["syFree"]],"workCleared":true})
            }
            _ => {
                return Err(Error::invalid(
                    "board operation out of order or blocked on external detached update",
                ))
            }
        };
        output.push(value);
    }
    Ok(json!({"callback":"ObjectBoard","frames":output}))
}
