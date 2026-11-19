//! Restricted kart finish task (CartD 0xd540), one scheduled update.
use crate::{kart_progress, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    mode: i8,
    selection: u8,
    timer: u16,
    flag_3ce24: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    state: State,
    input_words: [u32; 2],
    confirm_mask: u32,
}
#[derive(Serialize)]
pub struct Update {
    state: State,
    /// In invocation order. The task pointer is implicit for the delete request.
    calls: Vec<(&'static str, Option<u32>)>,
}
pub fn tick(rel: &[u8], input: Input) -> Result<Update> {
    kart_progress::verify(rel)?;
    let mut s = input.state;
    let buttons = input.input_words[0] | input.input_words[1];
    let mut calls = Vec::new();
    match s.mode {
        0 => {
            s.timer = s.timer.wrapping_add(1);
            if s.timer > 300 {
                s.timer = 0;
                s.mode = 1;
                s.selection = 1;
                s.flag_3ce24 = 1;
            }
        }
        1 if buttons & 0xc0 != 0 => {
            s.timer = 0;
            let choose_zero = buttons & 0x40 != 0;
            if choose_zero == (s.selection != 0) {
                s.selection = u8::from(!choose_zero);
                calls.push(("engine_8006b710", Some(0x8000)));
            }
        }
        1 if buttons & input.confirm_mask != 0 => {
            calls.push(("engine_8006b710", Some(0x8001)));
            if s.selection == 0 {
                calls.push(("engine_8001ef18", Some(4)));
                s.mode = 2;
            } else {
                calls.push(("engine_80071888_task", None));
            }
            s.flag_3ce24 = 1;
        }
        1 => {
            s.timer = s.timer.wrapping_add(1);
            if s.timer > 3600 {
                calls.push(("engine_80071888_task", None));
                s.flag_3ce24 = 1;
            }
        }
        _ => {}
    }
    Ok(Update { state: s, calls })
}
