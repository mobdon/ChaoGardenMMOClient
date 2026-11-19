//! Ground recovery after the first volley, up to the native mode-two handoff.
use crate::{Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Tick {
    Update,
    Heading(i32),
    Animation {
        motion: i32,
        frame: Option<f32>,
        ready: Option<bool>,
    },
}

pub(super) fn run(flags: u16, ticks: Vec<Tick>) -> Result<(Value, Option<u16>)> {
    if ticks.len() > 2048 {
        return Err(Error::invalid("too many ground recovery callbacks"));
    }
    let (mut mode, mut sub, mut timer) = (6, 0, 0u16);
    let mut rows = Vec::new();
    for t in ticks {
        let mut calls = Vec::new();
        match (mode, sub, t) {
            (6, 0, Tick::Update) => {
                timer = 120;
                sub = 1;
            }
            (6, 1, Tick::Heading(h)) => {
                calls.push(json!({"getHeading":0}));
                if !(-256..=256).contains(&h.wrapping_add(0x4000)) {
                    calls.push(json!({"motion":7}));
                    sub = 2;
                }
            }
            (
                6,
                2 | 3,
                Tick::Animation {
                    motion,
                    frame,
                    ready,
                },
            ) => {
                if (motion == 7) != frame.is_some()
                    || (sub == 3) != ready.is_some()
                    || frame.is_some_and(|f| !f.is_finite())
                {
                    return Err(Error::invalid(
                        "observations disagree with recovery animation queries",
                    ));
                }
                if let Some(f) = frame {
                    if (14.0..15.).contains(&f) {
                        calls.push(json!({"frameEvent":0}));
                    }
                    if (29.0..30.).contains(&f) {
                        calls.push(json!({"frameEvent":1}));
                    }
                }
                if sub == 2 {
                    calls.push(json!({"turnArgs":[128,0]}));
                    timer -= 1;
                    if timer == 0 {
                        sub = 3;
                    }
                } else {
                    calls.push(json!("animationQuery2d08"));
                    if ready == Some(true) {
                        timer = 40;
                        calls.push(json!({"motion":6}));
                        sub = 4;
                    }
                }
            }
            (6, 4, Tick::Update) => {
                timer -= 1;
                if timer == 0 {
                    mode = 2;
                    sub = 0;
                }
            }
            _ => {
                return Err(Error::invalid(
                    "ground recovery update does not match native state",
                ))
            }
        }
        rows.push(json!({"mode":mode,"submode":sub,"timer":timer,"flags":flags,"calls":calls}));
    }
    Ok((json!(rows), (mode == 2).then_some(flags)))
}
