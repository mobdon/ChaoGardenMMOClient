//! Embedded-Garden audio initialization and mode/control setup.
//! Register access and native10AE0 frequency/timer/scanline synchronization are
//! explicit strict dependencies. No mixer, sequencer or audio output is supplied.
use super::{
    frame::{FrameError, FrameMemory},
    garden_audio,
};

pub const INFO: u32 = 0x0300_61a0;
pub const PSG: u32 = 0x0300_71e0;
pub const CALLBACKS: u32 = 0x0300_7150;
pub const CODE_SOURCE: u32 = 0x0200_f9bc;
pub const CODE_DEST: u32 = 0x0300_3e44;
pub const CODE_BYTES: u32 = 0x400;
pub const INFO_BYTES: u32 = 0xfb0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    ReadRegister {
        address: u32,
        width: u8,
    },
    WriteRegister {
        address: u32,
        width: u8,
        value: u32,
    },
    /// Original10AE0 remains a required dependency, including its RAM effects.
    Frequency {
        mode: u32,
    },
    /// Payload10660's direct call to original F938. No mixer is supplied.
    Mixer,
    Player(garden_audio::Call),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Memory(FrameError),
    Audio(garden_audio::Error),
    Pointer(u32),
    Entry(u32),
    Arguments {
        entry: u32,
        actual: usize,
        expected: usize,
    },
    Unported(Call),
    Backend {
        call: Call,
        reason: String,
    },
}
impl From<FrameError> for Error {
    fn from(value: FrameError) -> Self {
        Self::Memory(value)
    }
}
impl From<garden_audio::Error> for Error {
    fn from(value: garden_audio::Error) -> Self {
        Self::Audio(value)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
pub trait Calls {
    /// Reads must return the actual bounded register value. Writes/frequency
    /// must not be acknowledged before their effects complete.
    fn invoke(&mut self, memory: &mut dyn FrameMemory, call: Call) -> Result<u32, Error>;
}
pub struct RejectCalls;
impl Calls for RejectCalls {
    fn invoke(&mut self, _: &mut dyn FrameMemory, call: Call) -> Result<u32, Error> {
        Err(Error::Unported(call))
    }
}
fn span(address: u32, length: u32) -> Result<(), Error> {
    address.checked_add(length).ok_or(Error::Pointer(address))?;
    Ok(())
}
fn word(m: &mut dyn FrameMemory, address: u32, value: u32) -> Result<(), Error> {
    m.write_u16(address, value as u16)?;
    m.write_u16(address + 2, (value >> 16) as u16)?;
    Ok(())
}
fn zero(m: &mut dyn FrameMemory, address: u32, words: u32) -> Result<(), Error> {
    span(address, words * 4)?;
    for index in 0..words {
        word(m, address + index * 4, 0)?;
    }
    Ok(())
}
fn read(m: &mut dyn FrameMemory, c: &mut dyn Calls, address: u32, width: u8) -> Result<u32, Error> {
    let value = c.invoke(m, Call::ReadRegister { address, width })?;
    Ok(match width {
        1 => value & 0xff,
        2 => value & 0xffff,
        _ => value,
    })
}
fn write(
    m: &mut dyn FrameMemory,
    c: &mut dyn Calls,
    address: u32,
    width: u8,
    value: u32,
) -> Result<(), Error> {
    c.invoke(
        m,
        Call::WriteRegister {
            address,
            width,
            value,
        },
    )?;
    Ok(())
}
fn info(m: &dyn FrameMemory) -> Result<u32, Error> {
    let address = m.read_u32(garden_audio::SOUND_INFO_SLOT)?;
    span(address, INFO_BYTES)?;
    Ok(address)
}
fn stop_dma(m: &mut dyn FrameMemory, c: &mut dyn Calls) -> Result<(), Error> {
    for address in [0x0400_00c4, 0x0400_00d0] {
        if read(m, c, address, 4)? & 0x0200_0000 != 0 {
            write(m, c, address, 4, 0x8440_0004)?;
        }
    }
    write(m, c, 0x0400_00c6, 2, 0x400)?;
    write(m, c, 0x0400_00d2, 2, 0x400)
}
/// Payload109E8, including FD C8's independent36-word callback table copy.
pub fn sound_init(m: &mut dyn FrameMemory, c: &mut dyn Calls, address: u32) -> Result<(), Error> {
    span(address, INFO_BYTES)?;
    word(m, address, 0)?;
    stop_dma(m, c)?;
    write(m, c, 0x0400_0084, 2, 0x8f)?;
    write(m, c, 0x0400_0082, 2, 0xa90e)?;
    let bias = (read(m, c, 0x0400_0089, 1)? & 0x3f) | 0x40;
    write(m, c, 0x0400_0089, 1, bias)?;
    write(m, c, 0x0400_00bc, 4, address + 0x350)?;
    write(m, c, 0x0400_00c0, 4, 0x0400_00a0)?;
    write(m, c, 0x0400_00c8, 4, address + 0x980)?;
    write(m, c, 0x0400_00cc, 4, 0x0400_00a4)?;
    word(m, garden_audio::SOUND_INFO_SLOT, address)?;
    zero(m, address, 0x3ec)?;
    m.write_u8(address + 6, 8)?;
    m.write_u8(address + 7, 15)?;
    word(m, address + 0x38, 0x0201_02b5)?;
    for offset in [0x28, 0x2c, 0x30, 0x3c] {
        word(m, address + offset, 0x0201_1ad9)?;
    }
    for index in 0..36 {
        let value = m.read_u32(0x0201_91c4 + index * 4)?;
        word(m, CALLBACKS + index * 4, value)?;
    }
    word(m, address + 0x34, CALLBACKS)?;
    c.invoke(m, Call::Frequency { mode: 0x40000 })?;
    word(m, address, garden_audio::SIGNATURE)
}
/// Payload108A4: native PSG register reset, callback rewiring and four records.
pub fn psg_init(m: &mut dyn FrameMemory, c: &mut dyn Calls, address: u32) -> Result<(), Error> {
    span(address, 0x100)?;
    for (register, width, value) in [
        (0x0400_0084, 2, 0x8f),
        (0x0400_0080, 2, 0),
        (0x0400_0063, 1, 8),
        (0x0400_0069, 1, 8),
        (0x0400_0079, 1, 8),
        (0x0400_0065, 1, 0x80),
        (0x0400_006d, 1, 0x80),
        (0x0400_007d, 1, 0x80),
        (0x0400_0070, 1, 0),
        (0x0400_0080, 1, 0x77),
    ] {
        write(m, c, register, width, value)?;
    }
    let sound = info(m)?;
    if m.read_u32(sound)? != garden_audio::SIGNATURE {
        return Ok(());
    }
    word(m, sound, garden_audio::SIGNATURE + 1)?;
    for (offset, target) in [
        (0x20, 0x0201_1861),
        (0x44, 0x0201_051d),
        (0x4c, 0x0201_0531),
        (0x70, 0x0201_19b9),
        (0x74, 0x0201_04b5),
        (0x78, 0x0201_0ae1),
        (0x7c, 0x0201_0241),
        (0x80, 0x0201_0ec5),
        (0x84, 0x0201_0f8d),
    ] {
        word(m, CALLBACKS + offset, target)?;
    }
    word(m, sound + 0x1c, address)?;
    for (offset, target) in [
        (0x28, 0x0201_11a1),
        (0x2c, 0x0201_10e9),
        (0x30, 0x0201_1041),
    ] {
        word(m, sound + offset, target)?;
    }
    m.write_u8(sound + 0xc, 0)?;
    zero(m, address, 0x40)?;
    for (offset, value) in [
        (1, 1),
        (0x1c, 0x11),
        (0x41, 2),
        (0x5c, 0x22),
        (0x81, 3),
        (0x9c, 0x44),
        (0xc1, 4),
        (0xdc, 0x88),
    ] {
        m.write_u8(address + offset, value)?;
    }
    word(m, sound, garden_audio::SIGNATURE)
}
/// Payload10C70. Its accepted signature values are SIGNATURE and SIGNATURE+1.
pub fn disable(m: &mut dyn FrameMemory, c: &mut dyn Calls) -> Result<(), Error> {
    let sound = info(m)?;
    let signature = m.read_u32(sound)?;
    if signature.wrapping_add(0x978c_92ad) > 1 {
        return Ok(());
    }
    word(m, sound, signature.wrapping_add(10))?;
    stop_dma(m, c)?;
    zero(m, sound + 0x350, 0x318)
}
/// Payload10CEC retains native behavior for every nonmatching signature.
pub fn enable(m: &mut dyn FrameMemory, c: &mut dyn Calls) -> Result<(), Error> {
    let sound = info(m)?;
    let signature = m.read_u32(sound)?;
    if signature == garden_audio::SIGNATURE {
        return Ok(());
    }
    write(m, c, 0x0400_00c6, 2, 0xb600)?;
    write(m, c, 0x0400_00d2, 2, 0xb600)?;
    let _ = m.read_u8(sound + 4)?;
    m.write_u8(sound + 4, 0)?;
    word(m, sound, signature.wrapping_sub(10))
}
/// Payload10B84. The native mode bits are preserved without inventing labels.
pub fn sound_mode(m: &mut dyn FrameMemory, c: &mut dyn Calls, mode: u32) -> Result<(), Error> {
    let sound = info(m)?;
    if m.read_u32(sound)? != garden_audio::SIGNATURE {
        return Ok(());
    }
    word(m, sound, garden_audio::SIGNATURE + 1)?;
    if mode & 0xff != 0 {
        m.write_u8(sound + 5, (mode & 0x7f) as u8)?;
    }
    if mode & 0xf00 != 0 {
        m.write_u8(sound + 6, ((mode & 0xf00) >> 8) as u8)?;
        for index in 0..12 {
            m.write_u8(sound + 0x50 + index * 0x40, 0)?;
        }
    }
    if mode & 0xf000 != 0 {
        m.write_u8(sound + 7, ((mode & 0xf000) >> 12) as u8)?;
    }
    if mode & 0x00b0_0000 != 0 {
        let value = (read(m, c, 0x0400_0089, 1)? & 0x3f) | ((mode & 0x0030_0000) >> 14);
        write(m, c, 0x0400_0089, 1, value)?;
    }
    if mode & 0x000f_0000 != 0 {
        disable(m, c)?;
        c.invoke(
            m,
            Call::Frequency {
                mode: mode & 0x000f_0000,
            },
        )?;
    }
    word(m, sound, garden_audio::SIGNATURE)
}
struct PlayerCalls<'a>(&'a mut dyn Calls);
impl garden_audio::Calls for PlayerCalls<'_> {
    fn call(
        &mut self,
        m: &mut dyn FrameMemory,
        call: garden_audio::Call,
    ) -> Result<(), garden_audio::Error> {
        self.0
            .invoke(m, Call::Player(call.clone()))
            .map(|_| ())
            .map_err(|e| match e {
                Error::Memory(error) => garden_audio::Error::Memory(error),
                Error::Audio(error) => error,
                error => garden_audio::Error::Backend {
                    call,
                    reason: error.to_string(),
                },
            })
    }
}
/// Payload105E8. Bank authenticates A3VE; mapped source bytes are copied in
/// native order and remain local. Player table priority flags are read live.
pub fn initialize(
    m: &mut dyn FrameMemory,
    c: &mut dyn Calls,
    bank: &garden_audio::Bank,
) -> Result<(), Error> {
    for index in 0..CODE_BYTES / 4 {
        let value = m.read_u32(CODE_SOURCE + index * 4)?;
        word(m, CODE_DEST + index * 4, value)?;
    }
    sound_init(m, c, INFO)?;
    psg_init(m, c, PSG)?;
    sound_mode(m, c, 0x0093_f400)?;
    for (index, player) in bank.players().iter().enumerate() {
        garden_audio::open(
            m,
            &mut PlayerCalls(c),
            player.context,
            player.tracks,
            u32::from(player.capacity),
        )?;
        let priority = m.read_u16(garden_audio::PLAYER_TABLE + index as u32 * 12 + 10)?;
        m.write_u8(player.context + 0xb, priority as u8)?;
        word(m, player.context + 0x18, 0x0300_73a0)?;
    }
    Ok(())
}
pub fn execute(
    m: &mut dyn FrameMemory,
    c: &mut dyn Calls,
    bank: &garden_audio::Bank,
    entry: u32,
    args: &[u32],
) -> Result<(), Error> {
    let expected = match entry {
        0x105e8 | 0x10660 | 0x10c70 | 0x10cec => 0,
        0x108a4 | 0x109e8 | 0x10b84 => 1,
        _ => return Err(Error::Entry(entry)),
    };
    if args.len() != expected {
        return Err(Error::Arguments {
            entry,
            actual: args.len(),
            expected,
        });
    }
    match entry {
        0x105e8 => initialize(m, c, bank),
        0x10660 => c.invoke(m, Call::Mixer).map(|_| ()),
        0x108a4 => psg_init(m, c, args[0]),
        0x109e8 => sound_init(m, c, args[0]),
        0x10b84 => sound_mode(m, c, args[0]),
        0x10c70 => disable(m, c),
        0x10cec => enable(m, c),
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../tests/garden-audio-init-vectors.json")).unwrap()
    }
    fn n(v: &Value, key: &str, default: u32) -> u32 {
        v.get(key)
            .and_then(Value::as_u64)
            .map_or(default, |v| v as u32)
    }
    struct Memory {
        ram: Vec<u8>,
        ewram: Vec<u8>,
        mmio: Vec<u8>,
    }
    impl Memory {
        fn parts(&self, address: u32) -> (&[u8], u32) {
            if address >= 0x0400_0000 {
                (&self.mmio, 0x0400_0000)
            } else if address >= 0x0300_0000 {
                (&self.ram, 0x0300_0000)
            } else {
                (&self.ewram, 0x0200_0000)
            }
        }
        fn get(&self, address: u32, width: usize) -> Result<&[u8], FrameError> {
            let (memory, base) = self.parts(address);
            address
                .checked_sub(base)
                .and_then(|at| memory.get(at as usize..(at as usize).checked_add(width)?))
                .ok_or(FrameError::MissingRead {
                    address,
                    width: width as u8,
                })
        }
        fn put(&mut self, address: u32, bytes: &[u8]) -> Result<(), FrameError> {
            let (memory, base) = if address >= 0x0400_0000 {
                (&mut self.mmio, 0x0400_0000)
            } else if address >= 0x0300_0000 {
                (&mut self.ram, 0x0300_0000)
            } else {
                (&mut self.ewram, 0x0200_0000)
            };
            let at = address.checked_sub(base).ok_or(FrameError::MissingWrite {
                address,
                width: bytes.len() as u8,
            })? as usize;
            memory
                .get_mut(at..at + bytes.len())
                .ok_or(FrameError::MissingWrite {
                    address,
                    width: bytes.len() as u8,
                })?
                .copy_from_slice(bytes);
            Ok(())
        }
        fn w(&mut self, address: u32, value: u32, width: usize) {
            self.put(address, &value.to_le_bytes()[..width]).unwrap();
        }
        fn new(source: &Value, case: &Value, payload: Option<&[u8]>) -> Self {
            let seed = n(case, "seed", 83);
            let mut m = Self {
                ram: (0..0x8000)
                    .map(|i| seed.wrapping_add(i * 37) as u8)
                    .collect(),
                ewram: vec![0; 0x40000],
                mmio: (0..0x1000).map(|i| (71u32 + i * 37) as u8).collect(),
            };
            if let Some(payload) = payload {
                m.put(0x0200_0000, payload).unwrap();
            }
            for (index, value) in source["callbackValues"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
            {
                m.w(
                    0x0201_91c4 + index as u32 * 4,
                    value.as_u64().unwrap() as u32,
                    4,
                );
            }
            for (index, row) in source["players"].as_array().unwrap().iter().enumerate() {
                let at = garden_audio::PLAYER_TABLE + index as u32 * 12;
                for (column, offset, width) in
                    [(0, 0, 4), (1, 4, 4), (2, 8, 1), (3, 9, 1), (4, 10, 2)]
                {
                    m.w(at + offset, row[column].as_u64().unwrap() as u32, width);
                }
            }
            m.w(garden_audio::SOUND_INFO_SLOT, INFO, 4);
            m.w(INFO, n(case, "signature", garden_audio::SIGNATURE), 4);
            m.w(0x0400_00c4, n(case, "dma1", 0), 4);
            m.w(0x0400_00d0, n(case, "dma2", 0), 4);
            m.w(0x0400_0089, n(case, "bias", 0xa5), 1);
            m
        }
    }
    impl FrameMemory for Memory {
        fn read_u8(&self, address: u32) -> Result<u8, FrameError> {
            Ok(self.get(address, 1)?[0])
        }
        fn read_u16(&self, address: u32) -> Result<u16, FrameError> {
            Ok(u16::from_le_bytes(
                self.get(address, 2)?.try_into().unwrap(),
            ))
        }
        fn read_u32(&self, address: u32) -> Result<u32, FrameError> {
            Ok(u32::from_le_bytes(
                self.get(address, 4)?.try_into().unwrap(),
            ))
        }
        fn write_u8(&mut self, address: u32, value: u8) -> Result<(), FrameError> {
            self.put(address, &[value])
        }
        fn write_u16(&mut self, address: u32, value: u16) -> Result<(), FrameError> {
            self.put(address, &value.to_le_bytes())
        }
    }
    struct Backend<'a> {
        case: &'a Value,
        calls: Vec<Value>,
        frequency: u32,
    }
    impl Backend<'_> {
        fn mutations(
            &self,
            m: &mut dyn FrameMemory,
            field: &str,
            key: &str,
            ordinal: u32,
        ) -> Result<(), Error> {
            if let Some(mutations) = self.case[field].as_array() {
                for v in mutations {
                    if n(v, key, 0) == ordinal {
                        let address = n(v, "address", 0);
                        let value = n(v, "value", 0);
                        match n(v, "width", 4) {
                            1 => m.write_u8(address, value as u8)?,
                            2 => m.write_u16(address, value as u16)?,
                            4 => word(m, address, value)?,
                            _ => panic!("fixture width"),
                        }
                    }
                }
            }
            Ok(())
        }
    }
    impl Calls for Backend<'_> {
        fn invoke(&mut self, m: &mut dyn FrameMemory, call: Call) -> Result<u32, Error> {
            let ordinal = self.calls.len() as u32 + 1;
            self.mutations(m, "mutations", "call", ordinal)?;
            let (event, value) = match call {
                Call::ReadRegister { address, width } => {
                    let value = match width {
                        1 => u32::from(m.read_u8(address)?),
                        2 => u32::from(m.read_u16(address)?),
                        4 => m.read_u32(address)?,
                        _ => panic!("register width"),
                    };
                    (
                        json!({"kind":"read","address":address,"width":width,"value":value}),
                        value,
                    )
                }
                Call::WriteRegister {
                    address,
                    width,
                    value,
                } => (
                    json!({"kind":"write","address":address,"width":width,"value":value}),
                    0,
                ),
                Call::Frequency { mode } => {
                    self.frequency += 1;
                    self.mutations(m, "frequencyMutations", "ordinal", self.frequency)?;
                    (json!({"kind":"frequency","mode":mode}), 0)
                }
                Call::Mixer => (json!({"kind":"mixer","entry":0xf938}), 0),
                Call::Player(_) => return Err(Error::Unported(call)),
            };
            self.calls.push(event);
            if ordinal == n(self.case, "abortAtCall", 0) {
                return Err(Error::Backend {
                    call,
                    reason: "synthetic interruption".into(),
                });
            }
            if let Call::WriteRegister {
                address,
                width,
                value,
            } = call
            {
                match width {
                    1 => m.write_u8(address, value as u8)?,
                    2 => m.write_u16(address, value as u16)?,
                    4 => word(m, address, value)?,
                    _ => panic!("register width"),
                }
            }
            Ok(value)
        }
    }
    fn check(
        source: &Value,
        case: &Value,
        payload: Option<&[u8]>,
        bank: Option<&garden_audio::Bank>,
    ) {
        let mut m = Memory::new(source, case, payload);
        let mut c = Backend {
            case,
            calls: vec![],
            frequency: 0,
        };
        let args: Vec<u32> = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap() as u32)
            .collect();
        let result = match n(case, "entry", 0) {
            0x105e8 => initialize(&mut m, &mut c, bank.unwrap()),
            0x109e8 => sound_init(&mut m, &mut c, args[0]),
            0x108a4 => psg_init(&mut m, &mut c, args[0]),
            0x10b84 => sound_mode(&mut m, &mut c, args[0]),
            0x10c70 => disable(&mut m, &mut c),
            0x10cec => enable(&mut m, &mut c),
            0x10660 => c.invoke(&mut m, Call::Mixer).map(|_| ()),
            _ => panic!("fixture entry"),
        };
        assert_eq!(
            result.is_err(),
            case["aborted"].as_bool().unwrap(),
            "{}: {result:?}",
            case["name"]
        );
        assert_eq!(json!(c.calls), case["calls"], "{}", case["name"]);
        assert_eq!(
            crate::sha256(&m.ram[..0x7400]),
            case["ramSha256"].as_str().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            crate::sha256(m.get(CODE_DEST, CODE_BYTES as usize).unwrap()),
            case["copiedCodeSha256"].as_str().unwrap(),
            "{}",
            case["name"]
        );
        if !case["aborted"].as_bool().unwrap() {
            assert_eq!(
                crate::sha256(&m.mmio),
                case["mmioSha256"].as_str().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
    #[test]
    fn native_setup_vectors_without_native_files() {
        let source = fixture();
        for case in source["cases"].as_array().unwrap() {
            if n(case, "entry", 0) != 0x105e8 {
                check(&source, case, None, None);
            }
        }
    }
    #[test]
    fn local_authenticated_init_vectors_when_supplied() {
        if let Ok(path) = std::env::var("SPP_GARDEN") {
            let payload = std::fs::read(path).unwrap();
            let bank = garden_audio::Bank::load(&payload).unwrap();
            let source = fixture();
            for case in source["cases"].as_array().unwrap() {
                if n(case, "entry", 0) == 0x105e8 {
                    check(&source, case, Some(&payload), Some(&bank));
                }
            }
        }
    }
    #[test]
    fn strict_registers_frequency_and_typed_errors() {
        let source = fixture();
        let case = &source["cases"][0];
        let mut m = Memory::new(&source, case, None);
        let error = sound_init(&mut m, &mut RejectCalls, INFO).unwrap_err();
        assert_eq!(
            error,
            Error::Unported(Call::ReadRegister {
                address: 0x0400_00c4,
                width: 4
            })
        );
        assert_eq!(m.read_u32(INFO).unwrap(), 0);
        let fault = FrameError::MissingRead {
            address: 0x1234,
            width: 4,
        };
        struct Fault(FrameError);
        impl Calls for Fault {
            fn invoke(&mut self, _: &mut dyn FrameMemory, _: Call) -> Result<u32, Error> {
                Err(Error::Memory(self.0.clone()))
            }
        }
        let call = garden_audio::Call::ClearPlayer {
            target: 0x0201_2491,
            player: 0x0300_72e0,
        };
        assert_eq!(
            garden_audio::Calls::call(&mut PlayerCalls(&mut Fault(fault.clone())), &mut m, call),
            Err(garden_audio::Error::Memory(fault))
        );
        assert_eq!(
            sound_init(&mut m, &mut RejectCalls, u32::MAX),
            Err(Error::Pointer(u32::MAX))
        );
    }
}
