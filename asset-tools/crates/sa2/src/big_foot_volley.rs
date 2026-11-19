//! Mode 8 on the uninterrupted opening. Child slots were filled during descent.
#[path = "big_foot_shot.rs"]
mod projectile;
use crate::{Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};

pub(super) struct Entry {
    pub flags: u16,
    pub angles: [i32; 3],
    pub anchors: [u32; 8],
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Launch {
    flight: Option<projectile::Replay>,
    random_result: i32,
    slot_zero_heading: i32,
    heading_difference: i32,
    player_position: [f32; 3],
}
pub(super) fn run(
    s: Entry,
    launches: [Launch; 8],
    frames: Option<Vec<Frame>>,
) -> Result<(Value, u16)> {
    if frames.as_ref().is_some_and(|f| f.len() > 2048)
        || (frames.is_some() && launches.iter().any(|l| l.flight.is_some()))
    {
        return Err(Error::invalid("invalid or competing volley timelines"));
    }
    let mut schedule = frames.map(Vec::into_iter);
    let mut slots: [Slot; 8] = std::array::from_fn(|_| Slot::Attached(None));
    let mut samples = launches.into_iter();
    let mut flags = s.flags;
    let (mut mode, mut sub, mut timer) = (8, 0, 0u16);
    let mut remaining = s.anchors.len();
    let mut burst = 0;
    let mut rows = Vec::new();
    while mode == 8 {
        let frame = schedule
            .as_mut()
            .map(|it| {
                it.next()
                    .ok_or_else(|| Error::invalid("missing volley frame"))
            })
            .transpose()?;
        let mut after = None;
        let mut before_rows = None;
        let mut seen = [false; 8];
        if let Some(frame) = frame {
            before_rows = Some(callbacks(frame.before, &mut slots, &s, &mut seen)?);
            after = Some(frame.after);
        }
        let mut shot = None;
        let mut motion = None;
        match sub {
            0 | 1 => {
                timer = 1;
                burst = remaining;
                sub = 2;
            }
            2 => {
                timer -= 1;
                if timer == 0 {
                    let obs = samples
                        .next()
                        .ok_or_else(|| Error::invalid("missing launch sample"))?;
                    if !obs.player_position.iter().all(|n| n.is_finite()) {
                        return Err(Error::invalid("invalid volley target"));
                    }
                    let spread =
                        (2048.0f32 - 4096.0 * (obs.random_result as f32 * (1.0 / 32768.0))) as i32;
                    remaining -= 1;
                    let offset = spread.wrapping_add(obs.heading_difference);
                    let mut child = json!({"slot":remaining,"anchorOffset":s.anchors[remaining],"angles":s.angles,
                        "target":obs.player_position,"differenceArgs":[obs.slot_zero_heading,s.angles[1]],
                        "headingOffset":offset,"speed":1.8f32,"mode":1,"submode":0});
                    if schedule.is_some() {
                        let Slot::Attached(Some(pos)) = slots[remaining] else {
                            return Err(Error::invalid("launch needs sampled attachment position"));
                        };
                        slots[remaining] = Slot::Launched(projectile::State::launch(
                            pos,
                            obs.player_position,
                            s.angles,
                            offset,
                        )?);
                    }
                    if let Some(flight) = obs.flight {
                        child["flight"] =
                            projectile::run(flight, obs.player_position, s.angles, offset)?;
                    }
                    shot = Some(child);
                    if remaining.is_multiple_of(4) {
                        timer = 45;
                        sub = 3;
                    } else {
                        timer = 15;
                    }
                }
            }
            3 => {
                timer -= 1;
                if timer == 0 {
                    if remaining == 0 {
                        motion = Some(10);
                        flags &= !0x100;
                        timer = 60;
                        sub = 4;
                    } else {
                        sub = 1;
                    }
                }
            }
            4 => {
                timer -= 1;
                if timer == 0 {
                    mode = 6;
                    sub = 0;
                }
            }
            _ => unreachable!(),
        }
        let mut row = json!({"mode":mode,"submode":sub,"timer":timer,"flags":flags,"remaining":remaining,"burst":burst,"launch":shot,"motion":motion});
        if let Some(after) = after {
            row["beforeBoss"] = json!(before_rows);
            row["afterBoss"] = json!(callbacks(after, &mut slots, &s, &mut seen)?);
        }
        rows.push(row);
    }
    if schedule.is_some_and(|mut it| it.next().is_some()) {
        return Err(Error::invalid("volley frames after recovery handoff"));
    }
    Ok((json!(rows), flags))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Frame {
    before: Vec<Callback>,
    after: Vec<Callback>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Callback {
    slot: usize,
    update: Update,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum Update {
    Attached([f32; 3]),
    Flight(projectile::Tick),
}
enum Slot {
    Attached(Option<[f32; 3]>),
    Launched(projectile::State),
}
fn callbacks(
    list: Vec<Callback>,
    slots: &mut [Slot; 8],
    boss: &Entry,
    seen: &mut [bool; 8],
) -> Result<Vec<Value>> {
    let mut rows = Vec::new();
    for Callback { slot, update } in list {
        let child = slots
            .get_mut(slot)
            .ok_or_else(|| Error::invalid("unknown child slot"))?;
        if std::mem::replace(&mut seen[slot], true) {
            return Err(Error::invalid("duplicate child callback"));
        }
        let row = match (child, update) {
            (Slot::Attached(last), Update::Attached(p)) if p.iter().all(|v| v.is_finite()) => {
                *last = Some(p);
                json!({"slot":slot,"anchorOffset":boss.anchors[slot],"position":p,"angles":boss.angles})
            }
            (Slot::Launched(state), Update::Flight(tick)) => {
                json!({"slot":slot,"flight":state.update(tick)?})
            }
            _ => return Err(Error::invalid("child callback disagrees with launch state")),
        };
        rows.push(row);
    }
    Ok(rows)
}
