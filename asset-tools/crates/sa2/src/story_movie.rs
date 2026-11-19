//! Stream readers for the nine verified PC story movies.
use crate::{manifest, Error, Result};
use serde_json::{json, Value};

const MOVIES: &[(i16, &str)] = &[
    (
        401,
        "73c243fb38cd0bca81904c08354f0b8dc481a7237c1ecb6383bc4aacd449f6c4",
    ),
    (
        409,
        "2f167b6ae7933d090f621cd0f7e97806ca101375d8b7f52ebdbae15af6e7b2de",
    ),
    (
        411,
        "34d44d60391a2c2c64bed7bf8464279384f78e511c201382b66b51c071121e31",
    ),
    (
        420,
        "e3646131be4b79f5327fa4c873a7a861063041f25d62b2a1f4f801fd2f47f0d8",
    ),
    (
        429,
        "4c5f1f171da70e4d8f6f017792ceaa6e2208869b5ea3fbfc887cf1cc36184f44",
    ),
    (
        430,
        "9cc34ae1ac7e24b6240f3a8da25f79b74a7ceed2675f9645b13e00f618e5613c",
    ),
    (
        524,
        "c1d750c4eb09fa37740011cb63053f373aced424546aeb17ec4b090860a17c5e",
    ),
    (
        532,
        "507557c1302bcdeb01bae138c173d9beaab7458ef5e07bf4a8b9db3cfaf5d786",
    ),
    (
        602,
        "64dc1ebc01d49f3622bbbfbca76f2e8ada27f0dae71be8e7bc147d12cecab32d",
    ),
];
pub fn supported(id: i16) -> bool {
    MOVIES.iter().any(|r| r.0 == id)
}
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
    demux_movie(602, b)
}
pub fn demux_movie(id: i16, b: &[u8]) -> Result<(Value, [Vec<u8>; 2], Vec<u8>)> {
    let expected = MOVIES
        .iter()
        .find(|r| r.0 == id)
        .ok_or_else(|| Error::invalid("unknown story movie ID"))?
        .1;
    let mut video = Vec::new();
    if manifest::sha256(b) != expected {
        return Err(Error::invalid("unverified story movie"));
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
                return Err(Error::invalid("unexpected story movie audio PES header"));
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
    let pictures = schedule.len();
    if schedule.windows(2).any(|w| w[0].0 == w[1].0) {
        report["presentationOrderUnavailable"] =
            json!("multiple pictures share a packet timestamp; picture timing unresolved");
    } else {
        report["presentationOrder"] = Value::Array(schedule.into_iter().map(|(_, p)| p).collect());
    }
    report["videoStream"] = json!({"bytes":video.len(),"sha256":manifest::sha256(video),"width":width,"height":height,"pictures":pictures,"sequenceRateCode":h[7]&15});
    Ok(())
}

/// Inspect one picture in presentation order without decoding the audio tracks.
pub fn picture_602(b: &[u8], frame: u32) -> Result<Value> {
    movie_picture(602, b, frame)
}
pub fn movie_picture(id: i16, b: &[u8], frame: u32) -> Result<Value> {
    let (mut report, _, video) = demux_movie(id, b)?;
    index_video(&mut report, &video)?;
    if report.get("presentationOrderUnavailable").is_some() {
        return Err(Error::invalid("movie picture timing unresolved"));
    }
    let pic = report["presentationOrder"]
        .get(frame as usize)
        .ok_or_else(|| Error::invalid("frame outside movie presentation order"))?;
    Ok(
        json!({"kind":"moviePicture","presentationIndex":frame,"picture":pic,"videoStream":report["videoStream"],"clockHz":report["clockHz"]}),
    )
}

pub fn inspect_602(b: &[u8]) -> Result<Value> {
    inspect_movie(602, b)
}
pub fn inspect_movie(id: i16, b: &[u8]) -> Result<Value> {
    let (mut report, tracks, video) = demux_movie(id, b)?;
    index_video(&mut report, &video)?;
    for (key, data) in ["c0", "c1"].into_iter().zip(tracks) {
        if data.is_empty() {
            continue;
        }
        let pcm = crate::audio::decode_adx(&data, 8_000_000)?;
        let bytes: Vec<u8> = pcm.samples.iter().flat_map(|v| v.to_le_bytes()).collect();
        report["audioTracks"][key] = json!({"header":pcm.header,"adxSha256":manifest::sha256(&data),"pcmS16leSha256":manifest::sha256(&bytes)});
    }
    Ok(report)
}
