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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayInput {
    mode: i8,
    selection: u8,
    places: [i32; 2],
    viewport_flag: bool,
}
/// Decode the original atlas and report submissions to the imported sprite renderer.
pub fn display(rel: &[u8], prs: &[u8], s: DisplayInput) -> Result<serde_json::Value> {
    use crate::{archive, city_rel::CityRel, manifest::sha256, texture, Error};
    kart_progress::verify(rel)?;
    if sha256(prs) != "91128a14218f5b67d089cddcefc769bf06387057097b619eece3a2261df3300b" {
        return Err(Error::invalid("unsupported resultCart.prs revision"));
    }
    let raw = archive::decompress_prs(prs, 4 * 1024 * 1024)?;
    let entries = archive::read_gvm(&raw)?;
    let [atlas] = entries.as_slice() else {
        return Err(Error::invalid("expected one kart atlas"));
    };
    let img = texture::decode_gvr(atlas.data, None)?;
    let module = CityRel::read_identified(rel, 80)?;
    let constants = module.data(4)?;
    let data = module.data(5)?;
    let f = |at: usize| f32::from_be_bytes(constants[at..at + 4].try_into().unwrap());
    let mut draws = Vec::new();
    let mut draw = |index: usize, x: usize, y: usize| {
        let at = 0x2ee0b8 + index * 20;
        let h = |i: usize| i16::from_be_bytes(data[at + i * 2..at + i * 2 + 2].try_into().unwrap());
        draws.push(serde_json::json!({"frame":index,"xy":[f(x),f(y)],"priority":f(0x468),
            "scale":[f(0x430),f(0x430)],"angle":0,"draw_flags":32,"size":[h(0),h(1)],
            "center":[h(0)/2,h(1)/2],"uv":[h(4),h(5),h(6),h(7)],"texture_id":h(8),"attribute":h(9)}));
    };
    match s.mode {
        0 => draw(
            1,
            0x464,
            if s.places[0] < s.places[1] {
                0x460
            } else {
                0x464
            },
        ),
        1 => {
            draw(2, 0x464, 0x44c);
            draw(if s.selection == 0 { 3 } else { 4 }, 0x470, 0x46c);
            draw(if s.selection == 0 { 6 } else { 5 }, 0x454, 0x46c);
        }
        _ => {}
    }
    Ok(
        serde_json::json!({"atlas":{"name":atlas.name,"width":img.width,"height":img.height,
        "rgba_sha256":sha256(&img.rgba)},"viewport_setup":s.viewport_flag,"draws":draws}),
    )
}
