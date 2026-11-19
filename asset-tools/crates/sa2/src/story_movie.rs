//! Packet inventory for the verified PC Last Story event 602 movie.
use crate::{manifest, Error, Result};
use serde_json::{json, Value};

/// Decode a marker-delimited 33-bit MPEG timestamp without reducing it to milliseconds.
fn timestamp(data: &[u8], prefix: u8) -> Result<u64> {
    let v = data
        .get(..5)
        .ok_or_else(|| Error::invalid("short MPEG timestamp"))?;
    if v[0] >> 4 != prefix || (v[0] & v[2] & v[4] & 1) == 0 {
        return Err(Error::invalid("invalid MPEG timestamp marker"));
    }
    Ok((u64::from(v[0] & 14) << 29)
        | (u64::from(v[1]) << 22)
        | (u64::from(v[2] & 254) << 14)
        | (u64::from(v[3]) << 7)
        | u64::from(v[4] >> 1))
}

/// Returns the original ADX streams in MPEG stream-ID order (C0, C1).
pub fn demux_602(b: &[u8]) -> Result<(Value, [Vec<u8>; 2])> {
    let (report, audio, _) = demux_av_602(b)?;
    Ok((report, audio))
}

/// Encoded elementary streams, before picture decoding or presentation scheduling.
pub fn demux_av_602(b: &[u8]) -> Result<(Value, [Vec<u8>; 2], Vec<u8>)> {
    let mut video = Vec::new();
    if manifest::sha256(b) != "64dc1ebc01d49f3622bbbfbca76f2e8ada27f0dae71be8e7bc147d12cecab32d" {
        return Err(Error::invalid("unverified event 602 movie"));
    }
    let mut at = 0;
    let mut audio: [Vec<u8>; 2] = Default::default();
    let mut timing = Vec::new();
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
            timing.push(json!({"stream":id,"packetOffset":at,"streamOffset":audio[usize::from(id-0xc0)].len(),"pts":timestamp(&packet[2..],2)?,"dts":null}));
            audio[usize::from(id - 0xc0)].extend_from_slice(&packet[7..]);
        }
        if id == 0xe0 {
            let packet = b
                .get(at + 6..at + len)
                .ok_or_else(|| Error::invalid("truncated movie video packet"))?;
            let mut header = packet
                .iter()
                .position(|&v| v != 0xff)
                .ok_or_else(|| Error::invalid("empty video packet"))?;
            if packet[header] & 0xc0 == 0x40 {
                header += 2;
            }
            header += match packet.get(header) {
                Some(0x0f) => 1,
                Some(v) if matches!(v >> 4, 2 | 3) => {
                    let prefix = v >> 4;
                    let pts = timestamp(&packet[header..], prefix)?;
                    let dts = if prefix == 3 {
                        Some(timestamp(&packet[header + 5..], 1)?)
                    } else {
                        None
                    };
                    timing.push(json!({"stream":id,"packetOffset":at,"streamOffset":video.len(),"pts":pts,"dts":dts}));
                    if dts.is_some() {
                        10
                    } else {
                        5
                    }
                }
                _ => return Err(Error::invalid("unexpected video PES header")),
            };
            video.extend_from_slice(
                packet
                    .get(header..)
                    .ok_or_else(|| Error::invalid("truncated video PES header"))?,
            );
        }
        at += len;
    }
    if !b[at..].iter().all(|v| *v == 255) {
        return Err(Error::invalid("unexpected movie end padding"));
    }
    Ok((
        json!({"container":"Sofdec","clockHz":90000,"timedPackets":timing,"packCount":counts[0xba],"videoPackets":counts[0xe0],"audioPackets":{"c0":counts[0xc0],"c1":counts[0xc1]},"endPaddingBytes":b.len()-at}),
        audio,
        video,
    ))
}

fn index_video(report: &mut Value, video: &[u8]) -> Result<()> {
    let h = video
        .get(..8)
        .filter(|v| v[..4] == [0, 0, 1, 0xb3])
        .ok_or_else(|| Error::invalid("expected MPEG sequence header"))?;
    let width = (u16::from(h[4]) << 4) | u16::from(h[5] >> 4);
    let height = (u16::from(h[5] & 15) << 8) | u16::from(h[6]);
    // A timed video PES anchors the next picture header. The final PES with
    // a repeated PTS carries no new picture and must not extend the schedule.
    let anchors: Vec<_> = report["timedPackets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["stream"] == 0xe0)
        .map(|p| {
            (
                p["streamOffset"].as_u64().unwrap() as usize,
                p["pts"].as_u64().unwrap(),
                p["packetOffset"].as_u64().unwrap(),
            )
        })
        .collect();
    let mut schedule = Vec::new();
    for (offset, h) in video
        .windows(6)
        .enumerate()
        .filter(|(_, v)| v[..4] == [0, 0, 1, 0])
    {
        let index = anchors
            .partition_point(|a| a.0 <= offset)
            .checked_sub(1)
            .ok_or_else(|| Error::invalid("unanchored movie picture"))?;
        let (_, pts, packet) = anchors[index];
        let kind = match (h[5] >> 3) & 7 {
            1 => "I",
            2 => "P",
            3 => "B",
            _ => return Err(Error::invalid("unsupported movie picture coding")),
        };
        schedule.push((pts,json!({"pts":pts,"packetOffset":packet,"streamOffset":offset,"codedIndex":schedule.len(),"coding":kind})));
    }
    schedule.sort_by_key(|a| a.0);
    if schedule.windows(2).any(|w| w[0].0 == w[1].0) {
        return Err(Error::invalid("ambiguous movie picture presentation time"));
    }
    let pictures = schedule.len();
    report["presentationOrder"] = Value::Array(schedule.into_iter().map(|(_, p)| p).collect());
    report["videoStream"] = json!({"bytes":video.len(),"sha256":manifest::sha256(video),"width":width,"height":height,"pictures":pictures,"sequenceRateCode":h[7]&15});
    Ok(())
}

/// Inspect one picture in presentation order without decoding the audio tracks.
pub fn picture_602(b: &[u8], frame: u32) -> Result<Value> {
    let (mut report, _, video) = demux_av_602(b)?;
    index_video(&mut report, &video)?;
    let pic = report["presentationOrder"]
        .get(frame as usize)
        .ok_or_else(|| Error::invalid("frame outside movie presentation order"))?;
    Ok(
        json!({"kind":"moviePicture","presentationIndex":frame,"picture":pic,"videoStream":report["videoStream"],"clockHz":report["clockHz"]}),
    )
}

pub fn inspect_602(b: &[u8]) -> Result<Value> {
    let (mut report, tracks, video) = demux_av_602(b)?;
    index_video(&mut report, &video)?;
    for (key, data) in ["c0", "c1"].into_iter().zip(tracks) {
        let pcm = crate::audio::decode_adx(&data, 3_000_000)?;
        let bytes: Vec<u8> = pcm.samples.iter().flat_map(|v| v.to_le_bytes()).collect();
        report["audioTracks"][key] = json!({"header":pcm.header,"adxSha256":manifest::sha256(&data),"pcmS16leSha256":manifest::sha256(&bytes)});
    }
    Ok(report)
}
