//! A3VE embedded Tiny Garden two-color board. Addresses in this module belong
//! to the decoded EWRAM program. No console reset, frame loop or save dispatch
//! is performed by these routines.
use super::frame::{FrameError, FrameMemory};

const ROUND: u32 = 0x0300_1ad1;
const RNG: u32 = 0x0300_13a8;
const BOARD: u32 = 0x0300_1ae8;
const MARKS: u32 = 0x0300_1b70;
const QUEUE: u32 = 0x0300_1df0;
const DIRECTIONS: [(i8, i8); 7] = [(1, -1), (0, -1), (-1, -1), (-1, 0), (-1, 1), (0, 1), (1, 1)];

/// The two native tables read at D5FE and D698. Keep their original contents
/// in the locally installed payload.
#[derive(Clone)]
pub struct BoardSetupTables {
    counts: [[u8; 3]; 4],
    tags: [u8; 3],
}
impl BoardSetupTables {
    pub fn from_payload(bytes: &[u8]) -> Result<Self, crate::garden::Error> {
        crate::garden::verify_payload_layout(bytes)?;
        let counts = std::array::from_fn(|round| {
            let offset = 0x2e108 + round * 3;
            [bytes[offset], bytes[offset + 1], bytes[offset + 2]]
        });
        Ok(Self {
            counts,
            tags: [bytes[0x2e114], bytes[0x2e115], bytes[0x2e116]],
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardError {
    Memory(FrameError),
    Round(u8),
    Position { row: u8, column: u8 },
    UnterminatedRay,
    UnknownCallback(u32),
    UnportedCall { entry: u32, args: Vec<u32> },
    UnportedEffect(BoardEffect),
}
impl From<FrameError> for BoardError {
    fn from(value: FrameError) -> Self {
        Self::Memory(value)
    }
}
impl std::fmt::Display for BoardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for BoardError {}

fn byte(memory: &dyn FrameMemory, address: u32) -> Result<u8, BoardError> {
    Ok(memory.read_u8(address)?)
}

fn read_bytes(memory: &dyn FrameMemory, address: u32, length: u32) -> Result<Vec<u8>, FrameError> {
    (0..length).map(|i| memory.read_u8(address + i)).collect()
}
fn random(seed: &mut u32) -> u16 {
    *seed = seed.wrapping_mul(0x41c6_4e6d).wrapping_add(12345);
    ((*seed >> 16) & 0x7fff) as u16
}

/// Native D504 through the instruction before D6C4. Clear the board, install
/// the top barrier, generate the alternating-color queue, then place the round's
/// three types of special cells. The final color run is written in full, even
/// when it crosses the nominal 200/300/400/500-cell end.
pub fn initialize_board_prefix(
    memory: &mut dyn FrameMemory,
    tables: &BoardSetupTables,
) -> Result<(), BoardError> {
    let round = byte(memory, ROUND)?;
    let mut counts = *tables
        .counts
        .get(usize::from(round))
        .ok_or(BoardError::Round(round))?;
    let mut seed = memory.read_u32(RNG)?;
    let mut board = [0u8; 11 * 12];
    board[1..=10].fill(0xf0);
    let nominal = 200 + 100 * usize::from(round);
    let mut queue = Vec::with_capacity(nominal + 3);
    let mut color = 1 + (random(&mut seed) & 1) as u8;
    while queue.len() < nominal {
        let count = usize::from((random(&mut seed) >> 4) & 3) + 1;
        queue.resize(queue.len() + count, color);
        color = 3 - color;
    }
    let specials: u16 = counts.iter().map(|&n| u16::from(n)).sum();
    // Position uses eight fractional bits, native BIOS Div truncates to an integer.
    let mut position = (20 + 10 * u16::from(round)) << 8;
    let step = position / specials; // Every authenticated round has at least one.
    let mut group = 0;
    for _ in 0..specials {
        if counts[group] == 0 {
            group += 1;
        }
        let offset =
            usize::from((position >> 8) - 1) * 10 + 1 + usize::from((random(&mut seed) >> 4) & 7);
        queue[offset] = tables.tags[group];
        counts[group] -= 1;
        position -= step;
    }
    memory.write_bytes(BOARD, &board)?;
    memory.write_bytes(0x0300_1ae6, &[0])?;
    memory.write_bytes(QUEUE, &queue)?;
    memory.write_bytes(0x0300_1ae4, &[specials as u8])?;
    memory.write_bytes(RNG, &seed.to_le_bytes())?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardEffect {
    /// Payload 0200E6D8, no arguments.
    RedrawMarks,
    /// Payload 02001698, native arguments (0x12, 0).
    QueueBg0VerticalOffset,
}
pub trait BoardEffects {
    fn invoke(
        &mut self,
        effect: BoardEffect,
        memory: &mut dyn FrameMemory,
    ) -> Result<(), BoardError>;
}
pub struct RejectBoardEffects;
impl BoardEffects for RejectBoardEffects {
    fn invoke(&mut self, effect: BoardEffect, _: &mut dyn FrameMemory) -> Result<(), BoardError> {
        Err(BoardError::UnportedEffect(effect))
    }
}

fn cell(row: u8, column: u8) -> u32 {
    u32::from(row) * 12 + u32::from(column)
}

/// Payload E974. Coordinates are byte registers; a real read past the logical
/// grid still uses the supplied IWRAM image, including its original aliases.
fn flip_ray(
    memory: &mut dyn FrameMemory,
    row: u8,
    column: u8,
    color: u8,
    (dy, dx): (i8, i8),
) -> Result<bool, BoardError> {
    let mut y = row;
    let mut x = column;
    for distance in 1..=256 {
        y = y.wrapping_add_signed(dy);
        x = x.wrapping_add_signed(dx);
        let found = byte(memory, BOARD + cell(y, x))? & 0x0f;
        if found == 0 {
            return Ok(false);
        }
        if found == color {
            if distance == 1 {
                return Ok(false);
            }
            let mut y = row;
            let mut x = column;
            for _ in 1..distance {
                y = y.wrapping_add_signed(dy);
                x = x.wrapping_add_signed(dx);
                let index = cell(y, x);
                memory.write_bytes(BOARD + index, &[color])?;
                memory.write_bytes(MARKS + index, &[0x18])?;
            }
            return Ok(true);
        }
    }
    // Byte-coordinate pairs repeat after 256 steps. A malformed RAM snapshot
    // with no terminator would otherwise spin forever in the original.
    Err(BoardError::UnterminatedRay)
}

/// Payload EA5C. Clear only the 10×10 marks interior. For each bracketed
/// nonmatching run, write the placed ball's color and mark it for animation.
/// Native R2 is the full origin byte; only scanned cells use the low nibble.
pub fn flip_bracketed_balls(
    memory: &mut dyn FrameMemory,
    row: u8,
    column: u8,
    effects: &mut dyn BoardEffects,
) -> Result<bool, BoardError> {
    if !(1..=10).contains(&row) || !(1..=10).contains(&column) {
        return Err(BoardError::Position { row, column });
    }
    for y in 1..=10 {
        memory.write_bytes(MARKS + cell(y, 1), &[0; 10])?;
    }
    let mut flipped = false;
    for direction in DIRECTIONS {
        let color = byte(memory, BOARD + cell(row, column))?;
        flipped |= flip_ray(memory, row, column, color, direction)?;
    }
    if flipped {
        memory.write_bytes(0x0300_2a30, &[0])?;
        effects.invoke(BoardEffect::RedrawMarks, memory)?;
        effects.invoke(BoardEffect::QueueBg0VerticalOffset, memory)?;
    }
    Ok(flipped)
}

/// Native dependencies use absolute decoded-payload instruction addresses.
/// Only 0D04 (object pointer) and 1698 (queue success) have a result in this API.
/// Void entries return zero, independent of the CPU register used for BX.
pub trait BoardCalls {
    fn invoke(
        &mut self,
        entry: u32,
        args: &[u32],
        memory: &mut dyn FrameMemory,
    ) -> Result<u32, BoardError>;
}
pub struct RejectBoardCalls;
impl BoardCalls for RejectBoardCalls {
    fn invoke(
        &mut self,
        entry: u32,
        args: &[u32],
        _: &mut dyn FrameMemory,
    ) -> Result<u32, BoardError> {
        Err(BoardError::UnportedCall {
            entry,
            args: args.to_vec(),
        })
    }
}

/// Concrete dependencies of D504. 16D8 and 12E0 also implement the first
/// two calls of D7CC against the caller's mapped MMIO and OAM. Remaining
/// entries report their absolute instruction address and argument values.
#[derive(Default)]
pub struct NativeBoardCalls;
impl BoardCalls for NativeBoardCalls {
    fn invoke(
        &mut self,
        entry: u32,
        args: &[u32],
        memory: &mut dyn FrameMemory,
    ) -> Result<u32, BoardError> {
        match (entry, args) {
            (0x0200_0c90, []) => {
                // 1290 initializes the sprite attribute and affine free lists.
                for i in 0..128 {
                    memory.write_bytes(0x0300_5d90 + i * 8, &0x200u16.to_le_bytes())?;
                }
                let affine = std::array::from_fn::<_, 32, _>(|i| (i * 2) as u8);
                memory.write_bytes(0x0300_1120, &affine)?;
                memory.write_bytes(0x0300_1140, &[0; 3])?;
                // Two sentinel objects and the circular queue of unused IDs.
                memory.write_bytes(0x0300_5320, &0x0200_13ddu32.to_le_bytes())?;
                memory.write_bytes(0x0300_5324, &[0, 0, 1])?;
                memory.write_bytes(0x0300_5344, &0x0200_0f31u32.to_le_bytes())?;
                memory.write_bytes(0x0300_5348, &[1, 0, 0])?;
                let free = std::array::from_fn::<_, 70, _>(|i| (i + 2) as u8);
                memory.write_bytes(0x0300_05c0, &free)?;
                memory.write_bytes(0x0300_0606, &[0; 3])?;
                memory.write_bytes(0x0300_6190, &[0])?;
                Ok(0)
            }
            (0x0200_0d04, &[callback, parent, insert_before]) => {
                let used = byte(memory, 0x0300_0606)?;
                if used > 0x45 {
                    return Ok(0);
                }
                let head = byte(memory, 0x0300_0607)?;
                let id = byte(memory, 0x0300_05c0 + u32::from(head))?;
                let next = head.wrapping_add(1);
                memory.write_bytes(0x0300_0607, &[if next > 0x45 { 0 } else { next }])?;
                let object = |id: u8| 0x0300_5320 + u32::from(id) * 0x24;
                let out = object(id);
                memory.write_bytes(out, &callback.to_le_bytes())?;
                memory.write_bytes(out + 4, &[id])?;
                // Linked-list field writes follow the native ordering even if
                // a caller supplies an aliased parent or neighboring object.
                if insert_before as u8 != 0 {
                    let previous = byte(memory, parent.wrapping_add(5))?;
                    memory.write_bytes(out + 5, &[previous])?;
                    let parent_id = byte(memory, parent.wrapping_add(4))?;
                    memory.write_bytes(out + 6, &[parent_id])?;
                    let previous = byte(memory, parent.wrapping_add(5))?;
                    memory.write_bytes(object(previous) + 6, &[id])?;
                    memory.write_bytes(parent.wrapping_add(5), &[id])?;
                } else {
                    let parent_id = byte(memory, parent.wrapping_add(4))?;
                    memory.write_bytes(out + 5, &[parent_id])?;
                    let next = byte(memory, parent.wrapping_add(6))?;
                    memory.write_bytes(out + 6, &[next])?;
                    let next = byte(memory, parent.wrapping_add(6))?;
                    memory.write_bytes(object(next) + 5, &[id])?;
                    memory.write_bytes(parent.wrapping_add(6), &[id])?;
                }
                memory.write_bytes(out + 0x16, &[0; 4])?;
                memory.write_bytes(out + 0xc, &[0; 2])?;
                memory.write_bytes(out + 8, &[0; 2])?;
                memory.write_bytes(out + 7, &[0])?;
                memory.write_bytes(out + 0x1c, &[0xff])?;
                memory.write_bytes(0x0300_0606, &[byte(memory, 0x0300_0606)?.wrapping_add(1)])?;
                Ok(out)
            }
            (0x0200_167c | 0x0200_1698, &[register, value]) => {
                if entry == 0x0200_167c {
                    memory.write_bytes(0x0300_13d0, &[0])?;
                }
                let count = byte(memory, 0x0300_13d0)?;
                if count > 7 {
                    return Ok(0);
                }
                let pair = 0x0300_13b0 + 4 * u32::from(count);
                memory.write_bytes(pair, &(register as u16).to_le_bytes())?;
                memory.write_bytes(0x0300_13d0, &[count + 1])?;
                memory.write_bytes(pair + 2, &(value as u16).to_le_bytes())?;
                Ok(u32::from(entry == 0x0200_1698))
            }
            (0x0200_16d8, []) => {
                let mut count = byte(memory, 0x0300_13d0)?;
                while count != 0 {
                    count -= 1;
                    memory.write_bytes(0x0300_13d0, &[count])?;
                    let pair = 0x0300_13b0 + 4 * u32::from(count);
                    let reg = memory.read_u16(pair)?;
                    let val = memory.read_u16(pair + 2)?.to_le_bytes();
                    memory.write_bytes(0x0400_0000 + u32::from(reg & !1), &val)?;
                }
                Ok(0)
            }
            (0x0200_12e0, []) => {
                let shadow = read_bytes(memory, 0x0300_5d90, 0x400)?;
                memory.write_bytes(0x0700_0000, &shadow)?;
                Ok(0)
            }
            _ => Err(BoardError::UnportedCall {
                entry,
                args: args.to_vec(),
            }),
        }
    }
}
/// D504 and D7CC are state callbacks stored as odd Thumb pointers.
pub fn dispatch_board(
    memory: &mut dyn FrameMemory,
    tables: &BoardSetupTables,
    entry: u32,
    calls: &mut dyn BoardCalls,
) -> Result<(), BoardError> {
    match entry {
        0x0200_d505 => initialize_board(memory, tables, calls),
        0x0200_d7cd => board_frame(memory, calls),
        _ => Err(BoardError::UnknownCallback(entry)),
    }
}

fn initialize_board(
    memory: &mut dyn FrameMemory,
    tables: &BoardSetupTables,
    calls: &mut dyn BoardCalls,
) -> Result<(), BoardError> {
    initialize_board_prefix(memory, tables)?;
    // D6C4..D6FA: two CpuSet halfword fills and one CpuFastSet word fill.
    for dest in [0x0300_1ff0, 0x0300_24f0] {
        memory.write_bytes(dest, &[0; 0x500])?;
    }
    for flag in [0x0300_29f5, 0x0300_29f4, 0x0300_2a0c] {
        memory.write_bytes(flag, &[0])?;
    }
    memory.write_bytes(0x0600_c000, &[0; 0x1000])?;
    calls.invoke(0x0200_0c90, &[], memory)?;
    for (offset, value) in [(0x1e, 0x100u16), (0x06, 0x100), (0x16, 0), (0x0e, 0)] {
        memory.write_bytes(0x0300_5d90 + offset, &value.to_le_bytes())?;
    }
    for (entry, kind) in [
        (0x0200_dfbd, None),
        (0x0200_e3b5, None),
        (0x0200_f6bd, Some(4)),
        (0x0200_f689, Some(0x10)),
    ] {
        let object = calls.invoke(0x0200_0d04, &[entry, 0x0300_5320, 0], memory)?;
        if let Some(kind) = kind {
            memory.write_bytes(object.wrapping_add(0x1a), &[kind])?;
        }
    }
    calls.invoke(0x0200_167c, &[0, 8000], memory)?;
    memory.write_bytes(0x0300_425f, &[1])?;
    memory.write_bytes(0x0300_4250, &0x0200_d7cdu32.to_le_bytes())?;
    Ok(())
}

fn board_frame(memory: &mut dyn FrameMemory, calls: &mut dyn BoardCalls) -> Result<(), BoardError> {
    calls.invoke(0x0200_16d8, &[], memory)?;
    calls.invoke(0x0200_12e0, &[], memory)?;
    // The three CpuFastSet calls copy from independent dirty buffers. Native
    // clears each flag after its copy.
    for (flag, src, dest, length) in [
        (0x0300_2a0c, 0x0300_2a10, 0x0500_01e0, 32),
        (0x0300_29f4, 0x0300_1ff0, 0x0600_c000, 0x500),
        (0x0300_29f5, 0x0300_24f0, 0x0600_c800, 0x500),
    ] {
        if byte(memory, flag)? != 0 {
            let bytes = read_bytes(memory, src, length)?;
            memory.write_bytes(dest, &bytes)?;
            memory.write_bytes(flag, &[0])?;
        }
    }
    for entry in [
        0x0200_1420,
        0x0200_1374,
        0x0200_0eac,
        0x0200_117c,
        0x0200_1088,
        0x0200_db28,
    ] {
        calls.invoke(entry, &[], memory)?;
    }
    if byte(memory, 0x0300_425f)? & 2 != 0 {
        let return_pointer = memory.read_u32(0x0300_4254)?;
        if return_pointer == 0 {
            calls.invoke(0x0200_1698, &[0x50, 0xbf], memory)?;
            calls.invoke(0x0201_0804, &[0x0300_72e0, 1], memory)?;
        } else {
            calls.invoke(0x0200_1698, &[0x50, 0xff], memory)?;
        }
        memory.write_bytes(0x0300_425c, &0u16.to_le_bytes())?;
        memory.write_bytes(0x0300_4250, &0x0200_f625u32.to_le_bytes())?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::super::memory::{MemoryImage, Region};
    use super::*;
    use serde_json::Value;

    const SNAPSHOT: u32 = 0x0300_1300;
    const SIZE: usize = 0x1734;

    fn fixture() -> Value {
        serde_json::from_str(vectors!("garden-payload-vectors.json")).unwrap()
    }
    fn ram() -> MemoryImage<'static> {
        MemoryImage::fixture(vec![Region {
            base: SNAPSHOT,
            writable: true,
            bytes: vec![0xa5; SIZE],
        }])
    }
    fn number(value: &Value) -> u32 {
        value.as_u64().unwrap().try_into().unwrap()
    }
    fn check_setup(tables: &BoardSetupTables, synthetic: bool) {
        for case in fixture()["setupCases"].as_array().unwrap() {
            if case["synthetic"].as_bool().unwrap() != synthetic {
                continue;
            }
            let mut memory = ram();
            memory
                .write_bytes(ROUND, &[number(&case["round"]) as u8])
                .unwrap();
            memory
                .write_bytes(RNG, &number(&case["seed"]).to_le_bytes())
                .unwrap();
            initialize_board_prefix(&mut memory, tables).unwrap();
            assert_eq!(
                crate::sha256(memory.bytes(SNAPSHOT, SIZE).unwrap()),
                case["ramSha256"],
                "{case}"
            );
        }
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn setup_matches_native_with_injected_tables() {
        let tables = BoardSetupTables {
            counts: [[1, 2, 3]; 4],
            tags: [0xd3, 0xe6, 0xa8],
        };
        check_setup(&tables, true);
        let mut memory = ram();
        assert_eq!(
            initialize_board_prefix(&mut memory, &tables),
            Err(BoardError::Round(0xa5))
        );
    }

    #[test]
    #[ignore = "requires local SPP_ROM for original Garden setup tables"]
    fn authentic_board_setup_matches_native_all_four_rounds() {
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(std::env::var_os("SPP_ROM").expect("set SPP_ROM"))
            .unwrap()
            .take(0x800001)
            .read_to_end(&mut bytes)
            .unwrap();
        crate::catalog::validate_rom(&bytes).unwrap();
        let payload = crate::binary::decode(&bytes[0xd0970..], crate::garden::PAYLOAD_LENGTH)
            .unwrap()
            .bytes;
        let tables = BoardSetupTables::from_payload(&payload).unwrap();
        check_setup(&tables, false);
        check_composed(&tables);
        check_concrete_setup(&tables);
        for source in fixture()["sources"].as_object().unwrap().values() {
            let offset = number(&source["offset"]) as usize;
            let length = number(&source["length"]) as usize;
            assert_eq!(
                crate::sha256(&payload[offset..offset + length]),
                source["sha256"]
            );
        }
    }

    #[derive(Default)]
    struct Effects(Vec<Value>);
    impl BoardEffects for Effects {
        fn invoke(
            &mut self,
            effect: BoardEffect,
            _: &mut dyn FrameMemory,
        ) -> Result<(), BoardError> {
            self.0.push(match effect {
                BoardEffect::RedrawMarks => serde_json::json!([0x0200_e6d8, []]),
                BoardEffect::QueueBg0VerticalOffset => serde_json::json!([0x0200_1698, [0x12, 0]]),
            });
            Ok(())
        }
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn bracketed_flips_match_native_ram_and_effect_order() {
        for case in fixture()["flipCases"].as_array().unwrap() {
            let mut memory = ram();
            memory.write_bytes(BOARD, &[0; 132]).unwrap();
            for input in case["cells"].as_array().unwrap() {
                let position = cell(number(&input[0]) as u8, number(&input[1]) as u8);
                memory
                    .write_bytes(BOARD + position, &[number(&input[2]) as u8])
                    .unwrap();
            }
            let row = number(&case["origin"][0]) as u8;
            let col = number(&case["origin"][1]) as u8;
            let mut effects = Effects::default();
            let flipped = flip_bracketed_balls(&mut memory, row, col, &mut effects).unwrap();
            assert_eq!(u32::from(flipped), number(&case["flipped"]), "{case}");
            assert_eq!(effects.0, *case["calls"].as_array().unwrap(), "{case}");
            assert_eq!(
                crate::sha256(memory.bytes(SNAPSHOT, SIZE).unwrap()),
                case["ramSha256"],
                "{case}"
            );
        }
    }
    fn full_ram() -> MemoryImage<'static> {
        let mut iwram = vec![0; 0x7000];
        iwram[0x1300..0x1300 + SIZE].fill(0xa5);
        MemoryImage::fixture(vec![
            Region {
                base: 0x0300_0000,
                bytes: iwram,
                writable: true,
            },
            Region {
                base: 0x0400_0000,
                bytes: vec![0x66; 0x400],
                writable: true,
            },
            Region {
                base: 0x0700_0000,
                bytes: vec![0x77; 0x400],
                writable: true,
            },
            Region {
                base: 0x0500_0000,
                bytes: vec![0x77; 0x400],
                writable: true,
            },
            Region {
                base: 0x0600_c000,
                bytes: vec![0x88; 0x1000],
                writable: true,
            },
        ])
    }
    fn full_hash(memory: &MemoryImage<'_>) -> String {
        crate::sha256(
            &[
                (0x0300_0000, 0x7000),
                (0x0500_0000, 0x400),
                (0x0600_c000, 0x1000),
            ]
            .into_iter()
            .flat_map(|(a, n)| memory.bytes(a, n).unwrap().iter().copied())
            .collect::<Vec<_>>(),
        )
    }
    #[derive(Default)]
    struct Calls {
        log: Vec<Value>,
        allocations: u32,
    }
    impl BoardCalls for Calls {
        fn invoke(
            &mut self,
            entry: u32,
            args: &[u32],
            _: &mut dyn FrameMemory,
        ) -> Result<u32, BoardError> {
            self.log.push(serde_json::json!([entry, args]));
            if entry == 0x0200_0d04 {
                let result = 0x0300_5400 + self.allocations * 0x24;
                self.allocations += 1;
                Ok(result)
            } else {
                Ok(0)
            }
        }
    }
    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn frame_matches_original_conditional_uploads_and_exit_branches() {
        for case in fixture()["frameCases"].as_array().unwrap() {
            let mut memory = full_ram();
            let exit = number(&case["exitKind"]);
            let flags = number(&case["flags"]);
            memory
                .write_bytes(0x0300_4250, &0x0200_d7cdu32.to_le_bytes())
                .unwrap();
            memory
                .write_bytes(0x0300_4254, &u32::from(exit == 2).to_le_bytes())
                .unwrap();
            memory
                .write_bytes(0x0300_425c, &0x1234u16.to_le_bytes())
                .unwrap();
            memory
                .write_bytes(0x0300_425f, &[if exit != 0 { 2 } else { 0 }])
                .unwrap();
            for (i, flag) in [0x0300_2a0c, 0x0300_29f4, 0x0300_29f5]
                .into_iter()
                .enumerate()
            {
                memory
                    .write_bytes(flag, &[((flags >> i) & 1) as u8])
                    .unwrap();
            }
            for (src, len, mult) in [
                (0x0300_2a10, 32, 3),
                (0x0300_1ff0, 0x500, 5),
                (0x0300_24f0, 0x500, 7),
            ] {
                let bytes: Vec<_> = (0..len).map(|n| (n * mult + 1) as u8).collect();
                memory.write_bytes(src, &bytes).unwrap();
            }
            let mut calls = Calls::default();
            board_frame(&mut memory, &mut calls).unwrap();
            assert_eq!(full_hash(&memory), case["sha256"], "{case}");
            assert_eq!(calls.log, *case["calls"].as_array().unwrap(), "{case}");
        }
    }

    fn check_composed(tables: &BoardSetupTables) {
        let fixture = fixture();
        let case = &fixture["setupIntoFrame"];
        let mut memory = full_ram();
        memory
            .write_bytes(ROUND, &[number(&case["round"]) as u8])
            .unwrap();
        memory
            .write_bytes(RNG, &number(&case["seed"]).to_le_bytes())
            .unwrap();
        let mut calls = Calls::default();
        dispatch_board(&mut memory, tables, 0x0200_d505, &mut calls).unwrap();
        let next = u32::from_le_bytes(memory.bytes(0x0300_4250, 4).unwrap().try_into().unwrap());
        dispatch_board(&mut memory, tables, next, &mut calls).unwrap();
        assert_eq!(full_hash(&memory), case["sha256"]);
        assert_eq!(calls.log, *case["calls"].as_array().unwrap());
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn strict_calls_stop_at_first_missing_dependency() {
        let mut memory = full_ram();
        let tables = BoardSetupTables {
            counts: [[1, 2, 3]; 4],
            tags: [0xd3, 0xe6, 0xa8],
        };
        memory.write_bytes(ROUND, &[0]).unwrap();
        let mut strict = RejectBoardCalls;
        assert_eq!(
            dispatch_board(&mut memory, &tables, 0x0200_d505, &mut strict),
            Err(BoardError::UnportedCall {
                entry: 0x0200_0c90,
                args: vec![]
            })
        );
        assert_eq!(memory.bytes(0x0600_c000, 0x1000).unwrap(), &[0; 0x1000]);
        assert_eq!(memory.bytes(0x0300_4250, 4).unwrap(), &[0; 4]);
        assert_eq!(
            dispatch_board(&mut memory, &tables, 0x0200_d7cd, &mut strict),
            Err(BoardError::UnportedCall {
                entry: 0x0200_16d8,
                args: vec![]
            })
        );
        assert_eq!(
            dispatch_board(&mut memory, &tables, 0x0200_f625, &mut strict),
            Err(BoardError::UnknownCallback(0x0200_f625))
        );
        memory.write_bytes(BOARD, &[0; 132]).unwrap();
        memory.write_bytes(BOARD + cell(5, 5), &[1, 2, 1]).unwrap();
        assert_eq!(
            flip_bracketed_balls(&mut memory, 5, 5, &mut RejectBoardEffects),
            Err(BoardError::UnportedEffect(BoardEffect::RedrawMarks))
        );
        assert_eq!(memory.bytes(BOARD + cell(5, 6), 1).unwrap(), &[1]);
        assert_eq!(memory.bytes(MARKS + cell(5, 6), 1).unwrap(), &[0x18]);
    }
    fn hardware_digest(memory: &MemoryImage<'_>) -> String {
        crate::sha256(
            &[
                (0x0300_0000, 0x7000),
                (0x0400_0000, 0x400),
                (0x0500_0000, 0x400),
                (0x0600_c000, 0x1000),
                (0x0700_0000, 0x400),
            ]
            .into_iter()
            .flat_map(|(a, n)| memory.bytes(a, n).unwrap().iter().copied())
            .collect::<Vec<_>>(),
        )
    }
    fn check_concrete_setup(tables: &BoardSetupTables) {
        for case in fixture()["concreteSetup"].as_array().unwrap() {
            let mut memory = full_ram();
            memory
                .write_bytes(ROUND, &[number(&case["round"]) as u8])
                .unwrap();
            memory
                .write_bytes(RNG, &number(&case["seed"]).to_le_bytes())
                .unwrap();
            let mut native = NativeBoardCalls;
            dispatch_board(&mut memory, tables, 0x0200_d505, &mut native).unwrap();
            assert_eq!(hardware_digest(&memory), case["afterSetup"], "{case}");
            let next = memory.read_u32(0x0300_4250).unwrap();
            assert_eq!(
                dispatch_board(&mut memory, tables, next, &mut native),
                Err(BoardError::UnportedCall {
                    entry: number(&case["firstFrameBoundary"]),
                    args: vec![]
                })
            );
            assert_eq!(hardware_digest(&memory), case["atBoundary"], "{case}");
        }
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn concrete_object_pool_and_register_queue_match_instructions() {
        let f = fixture();
        let mut native = NativeBoardCalls;
        for case in f["objectCases"].as_array().unwrap() {
            let mut memory = full_ram();
            native.invoke(0x0200_0c90, &[], &mut memory).unwrap();
            memory
                .write_bytes(
                    0x0300_0606,
                    &[number(&case["used"]) as u8, number(&case["head"]) as u8],
                )
                .unwrap();
            let result = native
                .invoke(
                    0x0200_0d04,
                    &[0x0200_abcd, 0x0300_5320, number(&case["before"])],
                    &mut memory,
                )
                .unwrap();
            assert_eq!(result, number(&case["result"]));
            assert_eq!(hardware_digest(&memory), case["sha256"]);
        }
        let mut memory = full_ram();
        let case = &f["queueCase"];
        for (cmd, expected) in case["calls"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["results"].as_array().unwrap())
        {
            let value = native
                .invoke(
                    0x0200_0000 + number(&cmd[0]),
                    &[number(&cmd[1]), number(&cmd[2])],
                    &mut memory,
                )
                .unwrap();
            if !expected.is_null() {
                assert_eq!(value, number(expected));
            }
        }
        native.invoke(0x0200_16d8, &[], &mut memory).unwrap();
        assert_eq!(hardware_digest(&memory), case["sha256"]);
    }
}
