//! Offline inspection of completed ASOE Tiny Garden JoyBus data exchanges.
use crate::{binary, catalog, Error, Result};

pub const SAVE_TAIL_OFFSET: usize = 0x58;
pub const MAX_PAYLOAD_BYTES: usize = 0x800;
pub const RECEIVE_STAGING: u32 = 0x03002780;
pub const SEND_SOURCE: u32 = 0x03003018;
pub const RECEIVE_DONE: u32 = 0x6f616843;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Direction {
    ToGarden,
    FromGarden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Command {
    pub raw: u32,
    pub direction: Direction,
    pub opaque: u8,
    pub halfwords: u16,
}
impl Command {
    pub fn parse(raw: u32) -> Result<Self> {
        let direction = match raw as u8 {
            0x2a => Direction::ToGarden,
            0x22 => Direction::FromGarden,
            _ => return Err(Error("expected a Garden length command".into())),
        };
        let halfwords = ((raw >> 16) as u16).swap_bytes();
        if halfwords == 0 || usize::from(halfwords) > MAX_PAYLOAD_BYTES / 2 {
            return Err(Error(
                "length does not describe a nonempty bounded Garden save tail".into(),
            ));
        }
        Ok(Self {
            raw,
            direction,
            opaque: (raw >> 8) as u8,
            halfwords,
        })
    }
    pub fn payload_bytes(self) -> usize {
        usize::from(self.halfwords) * 2
    }
}

/// A host-write event and the Garden reply for that event. Repeated hardware
/// reads of an unchanged transmit register are outside this representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Exchange {
    pub host_word: u32,
    pub garden_word: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferReport {
    pub command: Command,
    pub payload: Vec<u8>,
    pub retried_words: usize,
    pub rejected_words: usize,
    pub completion_input: u16,
}

fn expect(observed: u32, expected: u32) -> Result<()> {
    if observed != expected {
        return Err(Error(
            "captured reply disagrees with native JoyBus framing".into(),
        ));
    }
    Ok(())
}

fn finish(
    command: Command,
    payload: Vec<u8>,
    retries: usize,
    rejected: usize,
    reply: u32,
    is_last: bool,
) -> Result<TransferReport> {
    if !is_last || reply as u16 != 0x4b43 || payload.len() != command.payload_bytes() {
        return Err(Error(
            "invalid Garden completion, payload length or trailing exchanges".into(),
        ));
    }
    Ok(TransferReport {
        command,
        payload,
        retried_words: retries,
        rejected_words: rejected,
        completion_input: ((reply >> 16) as u16).swap_bytes(),
    })
}
fn sequence(raw: u32) -> usize {
    usize::from((raw as u16).swap_bytes())
}
fn data(raw: u32) -> [u8; 2] {
    ((raw >> 16) as u16).to_le_bytes()
}

/// The initial length-command event is supplied separately and is assumed to
/// have been echoed. Use audit_transcript to also validate that first echo.
pub fn audit_capture(command_word: u32, exchanges: &[Exchange]) -> Result<TransferReport> {
    let command = Command::parse(command_word)?;
    match command.direction {
        Direction::ToGarden => receive(command, exchanges),
        Direction::FromGarden => send(command, exchanges),
    }
}

pub fn audit_transcript(events: &[Exchange]) -> Result<TransferReport> {
    let (first, rest) = events
        .split_first()
        .ok_or_else(|| Error("empty Garden transcript".into()))?;
    expect(first.garden_word, first.host_word)?;
    audit_capture(first.host_word, rest)
}

fn receive(command: Command, exchanges: &[Exchange]) -> Result<TransferReport> {
    let mut payload = Vec::with_capacity(command.payload_bytes());
    let mut retries = 0;
    let mut rejected = 0;
    for (
        i,
        &Exchange {
            host_word,
            garden_word,
        },
    ) in exchanges.iter().enumerate()
    {
        if !payload.is_empty()
            && payload.len() == command.payload_bytes()
            && host_word == RECEIVE_DONE
        {
            return finish(
                command,
                payload,
                retries,
                rejected,
                garden_word,
                i + 1 == exchanges.len(),
            );
        }
        let n = sequence(host_word);
        if payload.is_empty() && n != 0 && host_word as u8 == 0x2a {
            if host_word != command.raw {
                return Err(Error(
                    "capture restarts with a different Garden length".into(),
                ));
            }
            expect(garden_word, host_word)?;
            retries += 1;
            continue;
        }
        let next = payload.len() / 2;
        if n == next {
            if next == usize::from(command.halfwords) {
                return Err(Error(
                    "extra Garden payload word after declared length".into(),
                ));
            }
            expect(garden_word, host_word)?;
            payload.extend(data(host_word));
        } else if next != 0 && n == next - 1 {
            expect(garden_word, host_word)?;
            payload[(next - 1) * 2..next * 2].copy_from_slice(&data(host_word));
            retries += 1;
        } else {
            expect(garden_word, 0)?;
            rejected += 1;
        }
    }
    Err(Error("Garden receive completion was not captured".into()))
}

fn send(command: Command, exchanges: &[Exchange]) -> Result<TransferReport> {
    let mut payload = Vec::with_capacity(command.payload_bytes());
    let mut previous = None;
    let mut retries = 0;
    let mut rejected = 0;
    for (
        i,
        &Exchange {
            host_word,
            garden_word,
        },
    ) in exchanges.iter().enumerate()
    {
        if let Some(last) = previous {
            if host_word != last {
                expect(garden_word, last)?;
                retries += 1;
                continue;
            }
            if payload.len() == command.payload_bytes() {
                return finish(
                    command,
                    payload,
                    retries,
                    rejected,
                    garden_word,
                    i + 1 == exchanges.len(),
                );
            }
        } else {
            match host_word as u8 {
                0x26 => {}
                0x22 if host_word == command.raw => {
                    expect(garden_word, host_word)?;
                    retries += 1;
                    continue;
                }
                0x22 => {
                    return Err(Error(
                        "capture restarts with a different Garden length".into(),
                    ))
                }
                _ => {
                    expect(garden_word, 0)?;
                    rejected += 1;
                    continue;
                }
            }
        }
        if sequence(garden_word) != payload.len() / 2 {
            return Err(Error("Garden send reply has incorrect sequence".into()));
        }
        payload.extend(data(garden_word));
        previous = Some(garden_word);
    }
    Err(Error("Garden send completion was not captured".into()))
}

#[derive(Debug, serde::Serialize)]
pub struct SourceSpan {
    pub name: &'static str,
    pub payload_offset: usize,
    pub bytes: usize,
    pub sha256: String,
}

/// Verify the cartridge and decoded program, then fingerprint the executable
/// paths used by this parser. Payload offsets are relative to EWRAM 0x02000000.
pub fn source_evidence(rom: &[u8]) -> Result<Vec<SourceSpan>> {
    let decoded = catalog::garden(rom)?;
    let mut results = Vec::new();
    for (name, offset, bytes) in [
        ("joybus_init", 0x43c, 0x100),
        ("joybus_irq", 0x630, 0x2d0),
        ("receive_setup", 0x900, 0x58),
        ("receive_word", 0x958, 0xb8),
        ("send_setup", 0xa10, 0x4c),
        ("send_word", 0xa5c, 0xb0),
        ("receive_copy_and_save", 0x2c02, 0x88),
    ] {
        results.push(SourceSpan {
            name,
            payload_offset: offset,
            bytes,
            sha256: binary::sha256(binary::range(&decoded.bytes, offset, bytes)?),
        });
    }
    Ok(results)
}
