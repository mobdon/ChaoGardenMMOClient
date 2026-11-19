//! Source-local Sonic and streetboard pose pair for an explicitly observed action/frame.
use crate::{
    archive, board_model, board_motion, city_rel::CityRel, manifest, model, object_table, pose,
    Error, Result,
};
use serde_json::{json, Value};
const MODEL_SHA: &str = "d8f90e6c724e709b8d3431e725a5db1d1a096bdf5118d2ecca9ad356e7862d99";

pub fn rig(prs: &[u8], model_id: u16) -> Result<model::Model> {
    if manifest::sha256(prs) != MODEL_SHA {
        return Err(Error::invalid("unsupported Sonic model archive revision"));
    }
    let mut data = archive::decompress_prs(prs, 2 * 1024 * 1024)?;
    // Directory offsets and all node transform fields are 32-bit big-endian words.
    // The generic node reader leaves geometry opaque; never decode mesh payloads this way.
    for word in data.as_chunks_mut::<4>().0 {
        word.reverse()
    }
    let mut root = None;
    for row in data.as_chunks::<8>().0 {
        let id = u32::from_le_bytes(row[..4].try_into().unwrap());
        if id == u32::MAX {
            break;
        }
        if id == u32::from(model_id) {
            root = Some(u32::from_le_bytes(row[4..].try_into().unwrap()));
            break;
        }
    }
    let root_offset = root.ok_or_else(|| Error::invalid("missing Sonic model directory entry"))?;
    let nodes = model::read_nodes_at(&data, root_offset, data.len())?;
    Ok(model::Model {
        root_offset,
        format: "sonicmdl-rig".into(),
        version: 0,
        nodes,
        attachments: vec![],
        metadata: vec![],
        labels: Default::default(),
        unknowns: vec!["Rig only; geometry is not decoded.".into()],
    })
}

pub fn sample(stage: &[u8], exe: &[u8], prs: &[u8], action: u16, frame: f64) -> Result<Value> {
    if !(121..=144).contains(&action) {
        return Err(Error::invalid(
            "expected Sonic board action 121 through 144",
        ));
    }
    let pe = object_table::verify_image(exe)?;
    let row_va = 0xa17c18 + 16 * u32::from(action);
    let at = pe.va_to_offset(row_va, 16)?;
    let r = model::Reader { data: exe };
    let motion_id = r.u16(at)?;
    let model_id = r.u16(at + 2)?;
    let bank = board_motion::rider_slots(stage)?;
    let rider = bank
        .get(&motion_id)
        .ok_or_else(|| Error::invalid("no stage default for rider animation"))?;
    let board_bank = if stage.starts_with(b"MZ") {
        board_motion::read_pc(stage)?
    } else {
        board_motion::read(&CityRel::read(stage)?)?
    };
    let board = board_bank
        .for_action(action as i16)
        .ok_or_else(|| Error::invalid("missing board motion"))?;
    if !frame.is_finite()
        || frame < 0.
        || frame >= f64::from(board.frame_count.min(rider.frame_count))
    {
        return Err(Error::invalid("frame outside paired motion durations"));
    }
    let rider_pose = pose::evaluate_model_pose(&rig(prs, model_id)?, Some(rider), frame)?;
    let street = board_model::extract(exe, board_model::BoardModel::Streetboard)?;
    let board_pose =
        pose::evaluate_model_pose(&model::read_model(&street.sa2mdl)?, Some(board), frame)?;
    Ok(
        json!({"action":action,"frame":frame,"animationRowVa":row_va,"motionSlot":motion_id,"modelSlot":model_id,
        "motionMode":r.u16(at+4)?,"nextAction":r.u16(at+6)?,"transitionSpeed":r.f32(at+8)?,"animationSpeed":r.f32(at+12)?,
        "motionAddressSpace":if stage.starts_with(b"MZ") {"pe_rva"} else {"rel_section_5"},"riderMotionOffset":rider.offset,"boardMotionOffset":board.offset,"boardSlot":action+24,"rider":rider_pose,"board":board_pose,"scope":"Single-player model-local stage defaults at a supplied animation frame. World movement, live overrides and transitions are external."}),
    )
}
