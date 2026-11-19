use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    process::{Child, Command, Stdio},
    time::Duration,
};

struct Host(Child);

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn exchange(addr: SocketAddr, method: &str, path: &str, body: &str) -> (u16, Vec<u8>) {
    let timeout = Duration::from_secs(5);
    let mut stream = TcpStream::connect_timeout(&addr, timeout).unwrap();
    stream.set_read_timeout(Some(timeout)).unwrap();
    stream.set_write_timeout(Some(timeout)).unwrap();
    write!(stream, "{method} {path} HTTP/1.1\r\nHost: {addr}\r\n").unwrap();
    if method == "POST" {
        write!(stream, "Origin: http://{addr}\r\nContent-Type: application/x-a2ne-intent\r\nContent-Length: {}\r\n", body.len()).unwrap();
    }
    write!(stream, "\r\n{body}").unwrap();
    let mut reply = Vec::new();
    stream.take(200_000).read_to_end(&mut reply).unwrap();
    let split = reply.windows(4).position(|b| b == b"\r\n\r\n").unwrap();
    let headers = std::str::from_utf8(&reply[..split]).unwrap();
    let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, reply[split + 4..].to_vec())
}

fn frame(addr: SocketAddr, method: &str, path: &str, body: &str) -> (Value, Vec<u8>) {
    let (status, bytes) = exchange(addr, method, path, body);
    assert_eq!(status, 200, "{method} {path}");
    assert_eq!(&bytes[..4], b"A2NF");
    let end = 8 + u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let metadata: Value = serde_json::from_slice(&bytes[8..end]).unwrap();
    assert_eq!(metadata["width"], 240);
    assert_eq!(metadata["height"], 160);
    assert_eq!(bytes.len() - end, 240 * 160 * 4);
    (metadata["state"].clone(), bytes[end..].to_vec())
}

#[test]
#[ignore = "requires the local verified A2NE_ROM; starts an ephemeral loopback host"]
fn fresh_loopback_session_advances_and_resets_to_exact_initial_pixels() {
    let mut host = Host(
        Command::new(env!("CARGO_BIN_EXE_a2ne-native"))
            .arg("serve")
            .arg(std::env::var_os("A2NE_ROM").unwrap())
            .arg("0")
            .arg("9")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut announcement = String::new();
    BufReader::new(host.0.stdout.as_mut().unwrap())
        .read_line(&mut announcement)
        .unwrap();
    let addr: SocketAddr = announcement
        .split_once("http://")
        .unwrap()
        .1
        .trim()
        .trim_end_matches('/')
        .parse()
        .unwrap();
    assert!(addr.ip().is_loopback());
    let (initial, initial_pixels) = frame(addr, "GET", "/frame", "");
    assert_eq!(initial["frame"], 0);
    assert_eq!(initial["level_id"], 9);
    assert_eq!(exchange(addr, "GET", "/cartridge.gba", "").0, 404);
    assert_eq!(exchange(addr, "POST", "/tick", "2").0, 400);
    assert_eq!(frame(addr, "GET", "/frame", "").0, initial);
    let (tick, _) = frame(addr, "POST", "/tick", "16");
    assert_eq!(tick["frame"], 1);
    assert_eq!(tick["player"]["held_input"], 16);
    assert_eq!(frame(addr, "POST", "/reset", ""), (initial, initial_pixels));
}
