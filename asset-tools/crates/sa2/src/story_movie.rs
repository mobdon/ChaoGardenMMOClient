//! Packet inventory for the verified PC Last Story event 602 movie.
use crate::{manifest, Error, Result};
use serde_json::{json, Value};

pub fn inspect_602(b: &[u8]) -> Result<Value> {
    if manifest::sha256(b) != "64dc1ebc01d49f3622bbbfbca76f2e8ada27f0dae71be8e7bc147d12cecab32d" {
        return Err(Error::invalid("unverified event 602 movie"));
    }
    let mut at = 0;
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
        at += len;
    }
    if !b[at..].iter().all(|v| *v == 255) {
        return Err(Error::invalid("unexpected movie end padding"));
    }
    Ok(
        json!({"container":"Sofdec","packCount":counts[0xba],"videoPackets":counts[0xe0],"audioPackets":{"c0":counts[0xc0],"c1":counts[0xc1]},"endPaddingBytes":b.len()-at}),
    )
}
