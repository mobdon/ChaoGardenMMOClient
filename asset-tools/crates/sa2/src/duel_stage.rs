//! Stage 42's registered per-frame routine (stg42D.rel code +0x3078) and the
//! SET wrap it always runs (+0x2B8C). Each frame the SET objects outside a
//! window around player 1 are moved by a whole period along X and marked for
//! respawn, so rings and meteors keep reappearing ahead of the runners.
//! Engine state (players, SET status entries, the game pointer at
//! 0x803AD5D0) is an input; the one engine call is recorded.
use crate::duel_platform;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageConstants {
    pub fall_y: f32,
    pub half_window: f32,
    pub start_x: f32,
    pub period: f32,
    /// Section 5 offset of the stage object list the setup routine installs.
    pub object_list: u32,
}
pub fn constants(rel_bytes: &[u8]) -> Result<StageConstants> {
    let rel = duel_platform::module(rel_bytes)?;
    let f = |pc| duel_platform::float(&rel, pc);
    Ok(StageConstants {
        fall_y: f(0x30fe)?,
        half_window: f(0x2bbe)?,
        start_x: f(0x2be2)?,
        period: f(0x2bd2)?,
        object_list: duel_platform::target(&rel, 0x2dee, 5)?,
    })
}

/// Object-list flags (+2) for each row of the stage list, read from the REL.
pub fn object_flags(rel_bytes: &[u8], c: &StageConstants) -> Result<Vec<i16>> {
    let rel = duel_platform::module(rel_bytes)?;
    let d = rel.data(5)?;
    let word = |at: usize| -> Result<u32> {
        d.get(at..at + 4)
            .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
            .ok_or_else(duel_platform::invalid)
    };
    // The list head is a 16-bit count followed by the row pointer.
    let count = (word(c.object_list as usize)? >> 16) as usize;
    let rows = match rel.local_pointer(5, c.object_list + 4)? {
        Some((5, n)) => n as usize,
        _ => return Err(duel_platform::invalid()),
    };
    if count > 4096 {
        return Err(duel_platform::invalid());
    }
    (0..count)
        .map(|i| Ok((word(rows + 16 * i)? & 0xffff) as i16))
        .collect()
}

/// One engine SET status entry (16 bytes at 0x801F5568 + 16 i).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SetEntry {
    /// +0 and +1.
    pub bytes: [u8; 2],
    /// +2.
    pub status: i16,
    /// The SET record's object ID (record +0, low 15 bits index the list).
    pub object_id: u16,
    /// The SET record's X (record +8).
    pub x: f32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StageFrame {
    /// 0x803AD5D0 is non-null and its +0xC word is zero.
    pub game_ready: bool,
    /// Player 1 and 2 object +0x14 (X) and +0x18 (Y); `null` for an empty slot.
    pub players: [Option<[f32; 2]>; 2],
    /// Player data +0x10 bit 0x4000 for each slot.
    pub player_flags: [bool; 2],
    pub entries: Vec<SetEntry>,
}

/// Run +0x3078 once. Returns the engine calls; `entries` are updated in place.
pub fn step(c: &StageConstants, flags: &[i16], f: &mut StageFrame) -> Result<Vec<Value>> {
    let mut calls = Vec::new();
    let [p1, p2] = f.players;
    if let (true, Some([_, y]), false, Some(_), false) =
        (f.game_ready, p1, f.player_flags[0], p2, f.player_flags[1])
    {
        // fcmpo/bge: request the fall-out only when y < -100.
        if y < c.fall_y {
            calls.push(json!({ "engine": "0x8003b694", "args": [0] }));
        }
    }
    let px = p1.ok_or_else(|| Error::invalid("SET wrap reads player 1"))?[0];
    let high = c.half_window + px;
    for e in &mut f.entries {
        let row = (e.object_id & 0x7fff) as usize;
        let list_flags = *flags
            .get(row)
            .ok_or_else(|| Error::invalid("SET object outside the object list"))?;
        if list_flags & 2 != 0 || e.status & 9 != 0 {
            continue;
        }
        // fcmpo/blt: skip while player 1 is before X 300.
        if px < c.start_x {
            continue;
        }
        let x = e.x;
        // cror lt|eq: inside when x <= high and x > px - window.
        if x <= high && x > px - c.half_window {
            continue;
        }
        loop {
            while e.x > high {
                e.x -= c.period;
            }
            if e.x < px - c.half_window {
                e.x += c.period;
                continue;
            }
            break;
        }
        e.status = -0x8000;
        e.bytes = [0, 0];
    }
    Ok(calls)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    frames: Vec<StageFrame>,
}
pub fn replay(rel_bytes: &[u8], request: &[u8]) -> Result<Value> {
    let c = constants(rel_bytes)?;
    let flags = object_flags(rel_bytes, &c)?;
    let req: Request = serde_json::from_slice(request)
        .map_err(|e| Error::invalid(format!("stage frame request: {e}")))?;
    let mut out = Vec::with_capacity(req.frames.len());
    for mut f in req.frames {
        let calls = step(&c, &flags, &mut f)?;
        out.push(json!({ "calls": calls, "entries": f.entries }));
    }
    Ok(json!({ "constants": c, "objectFlags": flags, "frames": out }))
}
