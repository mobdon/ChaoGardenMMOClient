//! Standalone loopback execution lab; no MMO endpoints or save writes.
use serde::Deserialize;
use sonic_advance_2_assets::{
    ROM_LEN, VerifiedRom, gba, native_audio,
    native_game::{MAX_REPLAY_FRAMES, NativeHarness, SUPPORTED_INPUT, VIEW_HEIGHT, VIEW_WIDTH},
    native_stage::{NativeStage, TERRAIN_LEVELS, is_course_act},
};
use std::{
    error::Error,
    fs::File,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    time::Duration,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const HTML: &str = include_str!("../native_host.html");
const SCRIPT: &str = include_str!("../native_host.js");
const STYLE: &str = include_str!("../native_host.css");

fn read_limited(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err("Input exceeds its local size limit".into());
    }
    Ok(bytes)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Replay {
    segments: Vec<Segment>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Segment {
    keys: u16,
    frames: usize,
}

fn replay_frames(bytes: &[u8]) -> Result<Vec<u16>> {
    let replay: Replay = serde_json::from_slice(bytes)?;
    if replay.segments.is_empty() || replay.segments.len() > 4096 {
        return Err("Replay needs 1..4096 input segments".into());
    }
    let mut frames = Vec::new();
    for segment in replay.segments {
        if segment.keys & !SUPPORTED_INPUT != 0
            || segment.frames == 0
            || segment.frames > MAX_REPLAY_FRAMES - frames.len()
        {
            return Err("Replay exceeds supported inputs or 36000 frames".into());
        }
        frames.resize(frames.len() + segment.frames, segment.keys);
    }
    Ok(frames)
}

fn run_replay(rom: &VerifiedRom<'_>, bytes: &[u8], level: u8) -> Result<()> {
    let frames = replay_frames(bytes)?;
    let mut game = NativeHarness::for_level(rom, level)?;
    let mut transitions = vec![game.snapshot()];
    let mut previous = (
        game.snapshot().callback,
        game.snapshot().player.char_state,
        game.snapshot().variant,
    );
    for keys in &frames {
        let state = game.tick(*keys)?;
        let next = (state.callback, state.player.char_state, state.variant);
        if next != previous || state.halted.is_some() || state.game_over.is_some() {
            transitions.push(state.clone());
        }
        previous = next;
        if state.halted.is_some() || state.game_over.is_some() {
            break;
        }
    }
    let state = game.snapshot();
    let pixels = game.render()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "kind": "native Rust execution slice; incomplete retail game",
            "rom_sha256": sonic_advance_2_assets::ROM_SHA256,
            "input_sha256": gba::sha256(bytes), "requested_frames": frames.len(),
            "final": state, "transitions": transitions,
            "rgba_sha256": gba::sha256(&pixels), "width": VIEW_WIDTH, "height": VIEW_HEIGHT,
        }))?
    );
    Ok(())
}

struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
}

fn read_before(
    stream: &mut TcpStream,
    buffer: &mut [u8],
    deadline: std::time::Instant,
) -> Result<usize> {
    let remaining = deadline
        .checked_duration_since(std::time::Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or("Local request deadline exceeded")?;
    stream.set_read_timeout(Some(remaining))?;
    Ok(stream.read(buffer)?)
}

fn request(stream: &mut TcpStream, host: &str) -> Result<Request> {
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    let mut bytes = Vec::new();
    let end;
    loop {
        let mut buffer = [0; 1024];
        let count = read_before(stream, &mut buffer, deadline)?;
        if count == 0 {
            return Err("Incomplete request".into());
        }
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            end = i + 4;
            break;
        }
        if bytes.len() > 8192 {
            return Err("Request headers exceed limit".into());
        }
    }
    if end > 8192 {
        return Err("Request headers exceed limit".into());
    }
    let header = std::str::from_utf8(&bytes[..end])?;
    let mut lines = header.split("\r\n");
    let line = lines.next().ok_or("Missing request line")?;
    let parts = line.split_whitespace().collect::<Vec<_>>();
    if parts.len() != 3 || parts[2] != "HTTP/1.1" {
        return Err("Expected HTTP/1.1".into());
    }
    let method = parts[0].to_owned();
    let path = parts[1].to_owned();
    let mut length = None;
    let mut found_host = false;
    let mut found_type = false;
    for line in lines.filter(|line| !line.is_empty()) {
        let (key, value) = line.split_once(':').ok_or("Invalid request header")?;
        let value = value.trim();
        if key.eq_ignore_ascii_case("host") {
            if found_host || value != host {
                return Err("Wrong loopback host".into());
            }
            found_host = true;
        } else if key.eq_ignore_ascii_case("origin") {
            if value != format!("http://{host}") {
                return Err("Wrong local origin".into());
            }
        } else if key.eq_ignore_ascii_case("transfer-encoding") {
            return Err("Transfer encoding is unsupported".into());
        } else if key.eq_ignore_ascii_case("content-length") {
            if length.is_some() {
                return Err("Duplicate content length".into());
            }
            length = Some(value.parse::<usize>()?);
        } else if key.eq_ignore_ascii_case("content-type") {
            if found_type || value != "application/x-a2ne-intent" {
                return Err("Expected bounded native input content type".into());
            }
            found_type = true;
        }
    }
    if !found_host {
        return Err("Missing loopback host".into());
    }
    let length = length.unwrap_or(0);
    if length > 4 || (method == "POST" && !found_type) || (method != "POST" && length != 0) {
        return Err("Invalid bounded native input".into());
    }
    while bytes.len() < end + length {
        let mut buffer = [0; 4];
        let count = read_before(
            stream,
            &mut buffer[..(end + length - bytes.len()).min(4)],
            deadline,
        )?;
        if count == 0 {
            return Err("Incomplete input body".into());
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    if bytes.len() != end + length {
        return Err("Pipelined requests are unsupported".into());
    }
    Ok(Request {
        method,
        path,
        body: bytes[end..].to_vec(),
    })
}

fn response(stream: &mut TcpStream, status: &str, content_type: &str, bytes: &[u8]) -> Result<()> {
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nCross-Origin-Resource-Policy: same-origin\r\nContent-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'\r\n\r\n",
        bytes.len()
    )?;
    stream.write_all(bytes)?;
    Ok(())
}

fn packet(game: &NativeHarness<'_>) -> Result<Vec<u8>> {
    let snapshot = game.snapshot();
    // Course act music IDs run 13..26; level indices reserve two intervening slots per zone.
    let stage_music = 13 + u16::from(snapshot.level_id / 4) * 2 + u16::from(snapshot.level_id % 4);
    let state = serde_json::to_vec(
        &serde_json::json!({ "state": snapshot, "sound_cues": native_audio::SUPPORTED_CUES, "stage_music":stage_music, "width": VIEW_WIDTH, "height": VIEW_HEIGHT }),
    )?;
    let mut bytes = b"A2NF".to_vec();
    bytes.extend_from_slice(&(state.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&state);
    bytes.extend_from_slice(&game.render()?);
    Ok(bytes)
}

fn serve(rom: &VerifiedRom<'_>, port: u16, level: u8) -> Result<()> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))?;
    let host = listener.local_addr()?.to_string();
    let mut game = NativeHarness::for_level(rom, level)?;
    let mut cues = std::collections::HashMap::<u16, Vec<u8>>::new();
    println!("A2NE native Rust execution slice: http://{host}/");
    println!("One local session. Left/right and A (Z/space). Ctrl+C stops the lab.");
    for stream in listener.incoming() {
        let mut stream = stream?;
        let result = (|| -> Result<()> {
            let request = request(&mut stream, &host)?;
            match (request.method.as_str(), request.path.as_str()) {
                ("GET", "/") => response(
                    &mut stream,
                    "200 OK",
                    "text/html; charset=utf-8",
                    HTML.as_bytes(),
                ),
                ("GET", "/host.js") => response(
                    &mut stream,
                    "200 OK",
                    "text/javascript; charset=utf-8",
                    SCRIPT.as_bytes(),
                ),
                ("GET", "/host.css") => response(
                    &mut stream,
                    "200 OK",
                    "text/css; charset=utf-8",
                    STYLE.as_bytes(),
                ),
                ("GET", path) if path.starts_with("/audio/") => {
                    let id = path
                        .strip_prefix("/audio/")
                        .and_then(|p| p.strip_suffix(".wav"))
                        .and_then(|v| v.parse::<u16>().ok())
                        .filter(|x| native_audio::SUPPORTED_CUES.contains(x));
                    if let Some(cue) = id {
                        if let std::collections::hash_map::Entry::Vacant(entry) = cues.entry(cue) {
                            let complete = native_audio::render_cue(rom, cue)?;
                            entry.insert(native_audio::wav(&complete.audio)?);
                        }
                        response(&mut stream, "200 OK", "audio/wav", &cues[&cue])
                    } else {
                        response(&mut stream, "404 Not Found", "text/plain", b"Unknown cue")
                    }
                }
                ("GET", "/frame") => response(
                    &mut stream,
                    "200 OK",
                    "application/octet-stream",
                    &packet(&game)?,
                ),
                ("POST", "/tick") => {
                    let input: u16 = std::str::from_utf8(&request.body)?.parse()?;
                    game.tick(input)?;
                    response(
                        &mut stream,
                        "200 OK",
                        "application/octet-stream",
                        &packet(&game)?,
                    )
                }
                ("POST", "/reset") if request.body.is_empty() => {
                    game = NativeHarness::for_level(rom, level)?;
                    response(
                        &mut stream,
                        "200 OK",
                        "application/octet-stream",
                        &packet(&game)?,
                    )
                }
                _ => response(
                    &mut stream,
                    "404 Not Found",
                    "text/plain",
                    b"Unknown execution-lab request",
                ),
            }
        })();
        if let Err(error) = result {
            let _ = response(
                &mut stream,
                "400 Bad Request",
                "text/plain; charset=utf-8",
                error.to_string().as_bytes(),
            );
        }
    }
    Ok(())
}

fn run(args: &[String]) -> Result<()> {
    if args.len() < 2 {
        return Err("Usage: a2ne-native stages ROM | serve ROM [PORT [LEVEL_ID]] | replay ROM INPUTS.json [LEVEL_ID]".into());
    }
    let bytes = read_limited(Path::new(&args[1]), ROM_LEN)?;
    let rom = VerifiedRom::parse(&bytes)?;
    match args[0].as_str() {
        "stages" if args.len() == 2 => {
            for id in 0..TERRAIN_LEVELS {
                let stage = NativeStage::for_level(&rom, id)?;
                let (width, height) = stage.dimensions();
                let mode = if is_course_act(id) { "course movement" } else { "terrain only" };
                println!("level {id:2}   {width} x {height}   spawn {:?}   {mode}", stage.spawn);
            }
            Ok(())
        }
        "serve" if (2..=4).contains(&args.len()) => serve(
            &rom,
            args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(42872),
            args.get(3).map(|s| s.parse()).transpose()?.unwrap_or(0),
        ),
        "replay" if (3..=4).contains(&args.len()) => {
            run_replay(&rom, &read_limited(Path::new(&args[2]), 256 * 1024)?, args.get(3).map(|s| s.parse()).transpose()?.unwrap_or(0))
        }
        _ => Err("Usage: a2ne-native stages ROM | serve ROM [PORT [LEVEL_ID]] | replay ROM INPUTS.json [LEVEL_ID]".into()),
    }
}

fn main() {
    if let Err(error) = run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        eprintln!("a2ne-native: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replay_rejects_unimplemented_actions_and_unbounded_time() {
        assert_eq!(
            replay_frames(br#"{"segments":[{"keys":17,"frames":2},{"keys":0,"frames":1}]}"#)
                .unwrap(),
            [17, 17, 0]
        );
        for value in [
            br#"{"segments":[{"keys":2,"frames":1}]}"#.as_slice(),
            br#"{"segments":[{"keys":0,"frames":36001}]}"#,
            br#"{"segments":[{"keys":0,"frames":0}]}"#,
            br#"{"segments":[],"claimed_rings":99}"#,
        ] {
            assert!(replay_frames(value).is_err());
        }
    }
}
