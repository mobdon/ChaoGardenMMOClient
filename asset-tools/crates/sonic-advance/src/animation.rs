//! Bounded linear representation of observed SA1-style sprite commands.
//!
//! This parses words and preserves operands. It does not execute animation,
//! follow branches, infer frame/OAM layouts, or assert reachability. The `-13`
//! constant is deliberately unsupported: observed frame records begin with a
//! nonnegative duration instead. Region-specific script offsets belong in the
//! fingerprinted catalog, not this format reader.
use crate::{Error, Result};

pub const MAX_COMMANDS: usize = 4096;
pub const MAX_SCRIPT_BYTES: usize = MAX_COMMANDS * 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    ShowFrame,
    Tiles,
    Palette,
    JumpBack,
    End,
    PlaySound,
    Hitbox,
    Translate,
    Opaque8,
    ChangeAnimation,
    Opaque10,
    SetPriority,
    Opaque12,
}

impl CommandKind {
    /// Terminals end this linear block. Jump/change operands remain opaque;
    /// they are never used as input offsets by this decoder.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::JumpBack | Self::End | Self::ChangeAnimation)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// Offset relative to the start of the supplied script.
    pub byte_offset: usize,
    /// Original little-endian word, including a frame duration or signed opcode.
    pub raw_opcode: u32,
    pub kind: CommandKind,
    /// Original argument words. Packed palette/hitbox/translation values and
    /// currently unnamed command fields are preserved without reinterpretation.
    pub operands: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationScript {
    pub commands: Vec<Command>,
    /// Exact bytes through the first complete terminal, excluding trailing data.
    pub consumed: usize,
}

fn command_layout(raw_opcode: u32) -> Result<(CommandKind, usize)> {
    use CommandKind::*;
    let signed = raw_opcode as i32;
    Ok(match signed {
        0..=i32::MAX => (ShowFrame, 1),
        -1 => (Tiles, 2),
        -2 => (Palette, 2),
        -3 => (JumpBack, 1),
        -4 => (End, 0),
        -5 => (PlaySound, 1),
        -6 => (Hitbox, 2),
        -7 => (Translate, 1),
        -8 => (Opaque8, 2),
        -9 => (ChangeAnimation, 1),
        -10 => (Opaque10, 3),
        -11 => (SetPriority, 1),
        -12 => (Opaque12, 1),
        _ => return Err(Error(format!("unsupported animation opcode {signed}"))),
    })
}

fn word(input: &[u8], cursor: usize, max_bytes: usize) -> Result<u32> {
    let end = cursor
        .checked_add(4)
        .ok_or_else(|| Error("animation cursor overflow".into()))?;
    if end > max_bytes {
        return Err(Error(
            "animation byte budget exhausted before terminal".into(),
        ));
    }
    let bytes = input
        .get(cursor..end)
        .ok_or_else(|| Error("truncated animation command".into()))?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// Parse up to a caller-selected byte and command budget, additionally limited
/// to 4096 commands and 64 KiB. Input may include following assets: only the
/// terminal-delimited prefix is read. Missing terminals, incomplete words,
/// unknown negative opcodes and exhausted budgets are errors.
pub fn parse_commands(
    input: &[u8],
    max_commands: usize,
    max_bytes: usize,
) -> Result<AnimationScript> {
    let command_limit = max_commands.min(MAX_COMMANDS);
    let byte_limit = max_bytes.min(MAX_SCRIPT_BYTES);
    let mut commands = Vec::new();
    let mut cursor = 0;
    for _ in 0..command_limit {
        let byte_offset = cursor;
        let raw_opcode = word(input, cursor, byte_limit)?;
        cursor += 4;
        let (kind, operand_count) = command_layout(raw_opcode)?;
        let mut operands = Vec::new();
        operands
            .try_reserve_exact(operand_count)
            .map_err(|_| Error("animation operand allocation failed".into()))?;
        for _ in 0..operand_count {
            operands.push(word(input, cursor, byte_limit)?);
            cursor += 4;
        }
        commands
            .try_reserve(1)
            .map_err(|_| Error("animation command allocation failed".into()))?;
        commands.push(Command {
            byte_offset,
            raw_opcode,
            kind,
            operands,
        });
        if kind.is_terminal() {
            return Ok(AnimationScript {
                commands,
                consumed: cursor,
            });
        }
    }
    Err(Error(
        "animation command budget exhausted before terminal".into(),
    ))
}
