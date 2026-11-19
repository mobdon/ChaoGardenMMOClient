//! Packet inventory for the verified PC Last Story event 602 movie.
use crate::{manifest, Error, Result};
use serde_json::{json, Value};

/// Returns the original ADX streams in MPEG stream-ID order (C0, C1).
pub fn demux_602(b: &[u8]) -> Result<(Value, [Vec<u8>; 2])> {
    if manifest::sha256(b) != "64dc1ebc01d49f3622bbbfbca76f2e8ada27f0dae71be8e7bc147d12cecab32d" {
        return Err(Error::invalid("unverified event 602 movie"));
    }
    let mut at = 0;
    let mut audio: [Vec<u8>; 2] = Default::default();
    let mut counts = [0u32; 256];
    loop {
        let h = b
            .get(at..at + 4)
            .filter(|h| h[..3] == [0, 0, 1])
            .ok_or_else(|| Error::invalid("missing movie packet prefix"))?;
        let id = h[3];
        counts[id as usize] += 1;
        if id == 0xb9 {
            at += 4;
            break;
        }
        let len = if id == 0xba {
            12
        } else {
            let len = b
                .get(at + 4..at + 6)
                .ok_or_else(|| Error::invalid("truncated movie packet"))?;
            6 + usize::from(u16::from_be_bytes([len[0], len[1]]))
        };
        if matches!(id, 0xc0 | 0xc1) {
            let packet = b
                .get(at + 6..at + len)
                .ok_or_else(|| Error::invalid("truncated movie audio packet"))?;
            // This build uses a two-byte STD buffer field followed by a five-byte PTS.
            if packet.len() < 7
                || packet[..2] != [0x40, 0x04]
                || packet[2] & 0xf1 != 0x21
                || packet[4] & 1 == 0
                || packet[6] & 1 == 0
            {
                return Err(Error::invalid("unexpected event 602 audio PES header"));
            }
            audio[usize::from(id - 0xc0)].extend_from_slice(&packet[7..]);
        }
        at += len;
    }
    if !b[at..].iter().all(|v| *v == 255) {
        return Err(Error::invalid("unexpected movie end padding"));
    }
    Ok((
        json!({"container":"Sofdec","packCount":counts[0xba],"videoPackets":counts[0xe0],"audioPackets":{"c0":counts[0xc0],"c1":counts[0xc1]},"endPaddingBytes":b.len()-at}),
        audio,
    ))
}

pub fn inspect_602(b: &[u8]) -> Result<Value> {
    let (mut report, tracks) = demux_602(b)?;
    for (key, data) in ["c0", "c1"].into_iter().zip(tracks) {
        let pcm = crate::audio::decode_adx(&data, 3_000_000)?;
        let bytes: Vec<u8> = pcm.samples.iter().flat_map(|v| v.to_le_bytes()).collect();
        report["audioTracks"][key] = json!({"header":pcm.header,"adxSha256":manifest::sha256(&data),"pcmS16leSha256":manifest::sha256(&bytes)});
    }
    Ok(report)
}
