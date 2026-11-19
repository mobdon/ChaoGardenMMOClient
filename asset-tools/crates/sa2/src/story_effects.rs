//! Battle effects bank. Layout reference: SA3D.SA2Event.Effects.
use sa2_asset_tools::{audio, Error, Result};
use serde_json::{json, Value};

// Only called with complete, fixed-stride records and constant field offsets.
fn word(r: &[u8], i: usize) -> u32 {
    u32::from_be_bytes(r[i..i + 4].try_into().unwrap())
}
fn short(r: &[u8], i: usize) -> u16 {
    u16::from_be_bytes([r[i], r[i + 1]])
}
fn float(r: &[u8], i: usize) -> Result<f32> {
    let f = f32::from_bits(word(r, i));
    if f.is_finite() {
        Ok(f)
    } else {
        Err(Error::invalid("nonfinite event effect"))
    }
}
fn vector(r: &[u8], i: usize) -> Result<[f32; 3]> {
    Ok([float(r, i)?, float(r, i + 4)?, float(r, i + 8)?])
}

fn records(
    data: &[u8],
    start: usize,
    count: usize,
    size: usize,
    decode: impl Fn(&[u8]) -> Result<Value>,
) -> Result<Value> {
    let mut rows = Vec::new();
    for (slot, r) in data[start..start + count * size]
        .chunks_exact(size)
        .enumerate()
    {
        if r.iter().all(|b| *b == 0) {
            continue;
        }
        let mut value = decode(r)?;
        value["slot"] = json!(slot);
        value["byteOffset"] = json!(start + slot * size);
        rows.push(value);
    }
    Ok(Value::Array(rows))
}

pub fn read(data: &[u8]) -> Result<Value> {
    // Also validates the full bank length before the fixed slices below.
    let mut out = audio::event_lighting(data)?;
    out["simpleParticles"] = records(data, 43008, 2048, 56, |r| {
        Ok(
            json!({"frame":word(r,0),"type":r[4],"motionId":r[5],"textureId":float(r,8)?,"pulseControl":float(r,12)?,"unknown":float(r,16)?,"scale":float(r,20)?}),
        )
    })?;
    out["blareEffects"] = records(data, 227328, 64, 64, |r| {
        Ok(
            json!({"frame":word(r,0),"duration":word(r,4) as i32,"modelIndices":&r[8..14],"ghostLifespan":word(r,16) as i32}),
        )
    })?;
    out["particleEmitters"] = records(data, 231424, 64, 64, |r| {
        Ok(
            json!({"position":vector(r,0)?,"unknown2":vector(r,12)?,"unknown3To6":[short(r,24),short(r,26),short(r,28),short(r,30)],"frame":word(r,32),"spread":vector(r,36)?,"count":word(r,48) as i32,"unknown9":word(r,52) as i32,"type":word(r,56) as i32,"unknown11":word(r,60) as i32}),
        )
    })?;
    out["videoOverlays"] = records(data, 235520, 64, 64, |r| {
        let name = r[16..].split(|b| *b == 0).next().unwrap_or_default();
        Ok(
            json!({"frame":word(r,0),"position":[short(r,4) as i16,short(r,6) as i16],"depth":float(r,8)?,"type":r[12],"targetTextureId":r[13],"filename":std::str::from_utf8(name).map_err(|_|Error::invalid("invalid overlay filename"))?}),
        )
    })?;
    Ok(out)
}
