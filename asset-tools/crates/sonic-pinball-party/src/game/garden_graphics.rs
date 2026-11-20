//! Embedded A3VE Tiny Garden static scene setup and its original map helpers.
//!
//! Offsets and pointers in this module belong to the decoded Garden payload,
//! loaded at 0x02000000. They never select cartridge-ROM routines. BIOS
//! CpuFastSet/LZ77 transfers are represented by bounded memory copies and the
//! independent bounded decoder; this does not execute the GBA BIOS or PPU.
use super::frame::{FrameError, FrameMemory, NativeCall};

pub const PAYLOAD_BASE: u32 = 0x0200_0000;
pub const PAYLOAD_LENGTH: usize = 0x2e12c;
pub const PAYLOAD_SHA256: &str = "33dcbab013c9007ad838d8cbff693eef140768ca4110c9617c4055aa75659efb";
pub const MAX_RESOURCE_BYTES: usize = 0x40000;
pub(crate) fn diagnostic_address(offset: u32) -> u32 {
    if (offset as usize) < PAYLOAD_LENGTH {
        PAYLOAD_BASE + offset
    } else {
        offset
    }
}
pub const SOURCE_SPANS: [(&str, usize, usize, &str); 3] = [
    (
        "registerScript",
        0x163c,
        0x34,
        "b072578615dc6e52b9acc3cc116b5807560ecabb5f19376ff2982e2ae48488be",
    ),
    (
        "mapStamp",
        0x1718,
        0x48,
        "1ea0a9a7a33516d2b16df64a8f24309fc9dbfa4d70870ea0d51d93b73b6ae9e8",
    ),
    (
        "initialize",
        0xd294,
        0x270,
        "d43e126197c66c6f15fbe3927eff5c0f6f76a1ff491c772339dfe158c6b0a7aa",
    ),
];
pub const RESOURCE_SPANS: [(&str, usize, usize, &str); 11] = [
    (
        "objectPalette",
        0x1606c,
        224,
        "15927abd5d354edf7c87c1282f4b55c124430d985bf935c5945453181a7331d9",
    ),
    (
        "objectTilesLz77",
        0x1614c,
        3878,
        "c187d348641b37fdf9afcbf067d32c1330cd8efcbe47749d781fec7bab4798d0",
    ),
    (
        "backgroundPalette",
        0x17074,
        416,
        "1dfb6add24d0c66b29e5b5a27d9cf88599017a198495e5d7168ac6c873f6d33a",
    ),
    (
        "backgroundTilesLz77",
        0x17214,
        8109,
        "d3a6c072ea314f12db73fac21ab64048dc1f50a8af1ba9e666dde3a8c5576187",
    ),
    (
        "mapFirst",
        0x2dd64,
        60,
        "063a5c750a0de44d16cd341810bc6739f6691770627d7cc806b4c301c3c4e1e9",
    ),
    (
        "mapFirstAlternate",
        0x2dda0,
        24,
        "b68b2112574772105da1fa222b10c1f5b5be3e911aef8bb45e6b10e39132c465",
    ),
    (
        "mapSecond",
        0x2ddb8,
        54,
        "c8334e27ff972e99363acfda5aa0bdad9b3f14a698d6d0bf216f292c5a6cc880",
    ),
    (
        "mapThird",
        0x2ddee,
        48,
        "762fc0d0f51e603198a2de182b59cb94c935a6537feaad1ed071f2e88304e99e",
    ),
    (
        "mapLeft",
        0x2de1e,
        80,
        "40c615278c72c84cf1b8b525deb1127057f2bcc36dda9d01d341efaa5f8c1487",
    ),
    (
        "mapRight",
        0x2de6e,
        320,
        "ff1cb604b6ef3f74c46e87591fb67b6222cc1fc8d2297672bc0b0f47a12304ff",
    ),
    (
        "registerTable",
        0x2e074,
        64,
        "806901e0ce2ef77e5e3e24b880f1048322c994371a61005195170af556fbed4a",
    ),
];
pub const DECODED_TILES: [(u32, usize, &str); 2] = [
    (
        0x0201_7214,
        17760,
        "9c0387ae7e61eb585ac7d78bcf138bcae5684dae9097ab6d8fba9b037440c56b",
    ),
    (
        0x0201_614c,
        10848,
        "7fe825cbf93cad4f3344c14b80e33188947c2d49d9995e62608bb3cf14ccf0b9",
    ),
];
pub const GARDEN_FLAG: u32 = 0x0300_1ad0;
pub const SAVED_FLAG: u32 = 0x0300_4260;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Memory(FrameError),
    Binary(crate::binary::Error),
    PayloadIdentity,
    ResourceLimit(usize),
    ResourceLength {
        address: u32,
        expected: usize,
        actual: usize,
    },
    AddressOverflow {
        address: u32,
        displacement: u32,
    },
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
impl From<FrameError> for Error {
    fn from(value: FrameError) -> Self {
        Self::Memory(value)
    }
}
pub(crate) fn frame_error(offset: u32, error: Error) -> FrameError {
    match error {
        Error::Memory(error) => error,
        error => FrameError::Dependency {
            call: NativeCall {
                entry_offset: PAYLOAD_BASE + offset,
                argument0: None,
            },
            reason: error.to_string(),
        },
    }
}
impl From<crate::binary::Error> for Error {
    fn from(value: crate::binary::Error) -> Self {
        Self::Binary(value)
    }
}

/// Native Garden-payload call domain. An empty argument slice records only
/// that this caller prepared no arguments; it does not invent register values.
pub trait Calls {
    fn call(
        &mut self,
        memory: &mut dyn FrameMemory,
        offset: u32,
        args: &[u32],
    ) -> Result<(), FrameError>;
}
pub struct RejectCalls;
impl Calls for RejectCalls {
    fn call(
        &mut self,
        _: &mut dyn FrameMemory,
        offset: u32,
        args: &[u32],
    ) -> Result<(), FrameError> {
        Err(FrameError::Dependency {
            call: NativeCall {
                entry_offset: offset,
                argument0: args.first().copied(),
            },
            reason: "unported embedded Garden payload call".into(),
        })
    }
}

/// Resource pointers belong to the immutable decoded Garden payload. Hosts
/// supplying test doubles must still return the exact requested byte count.
pub trait Resources {
    fn read_bytes(&self, address: u32, length: usize) -> Result<Vec<u8>, Error>;
    fn decode_lz77(&mut self, address: u32) -> Result<Vec<u8>, Error>;
}
#[derive(Clone)]
pub struct GardenPayload<'a> {
    payload: &'a [u8],
}
impl<'a> GardenPayload<'a> {
    pub fn new(payload: &'a [u8]) -> Result<Self, Error> {
        if payload.len() != PAYLOAD_LENGTH || crate::sha256(payload) != PAYLOAD_SHA256 {
            return Err(Error::PayloadIdentity);
        }
        Ok(Self { payload })
    }
}
impl Resources for GardenPayload<'_> {
    fn read_bytes(&self, address: u32, length: usize) -> Result<Vec<u8>, Error> {
        if length > MAX_RESOURCE_BYTES {
            return Err(Error::ResourceLimit(length));
        }
        let offset = crate::binary::pointer_offset(address, PAYLOAD_BASE, self.payload.len())?;
        Ok(crate::binary::span(self.payload, offset, length)?.to_vec())
    }
    fn decode_lz77(&mut self, address: u32) -> Result<Vec<u8>, Error> {
        let offset = crate::binary::pointer_offset(address, PAYLOAD_BASE, self.payload.len())?;
        if self.payload[offset] != 0x10 {
            return Err(crate::binary::Error::InvalidTag(self.payload[offset]).into());
        }
        Ok(crate::binary::decode(&self.payload[offset..], MAX_RESOURCE_BYTES)?.bytes)
    }
}

fn at(address: u32, displacement: u32) -> Result<u32, Error> {
    address
        .checked_add(displacement)
        .ok_or(Error::AddressOverflow {
            address,
            displacement,
        })
}
fn exact(bytes: Vec<u8>, address: u32, length: usize) -> Result<Vec<u8>, Error> {
    if bytes.len() != length {
        return Err(Error::ResourceLength {
            address,
            expected: length,
            actual: bytes.len(),
        });
    }
    Ok(bytes)
}

/// Payload 0x163c: u8-counted pairs of (u16 MMIO offset, u16 value).
/// Reads and writes occur in native order; every address needs a host binding.
pub fn apply_register_script(
    memory: &mut dyn FrameMemory,
    source: u32,
    count: u32,
) -> Result<(), Error> {
    for index in 0..u32::from(count as u8) {
        let source = at(source, index * 4)?;
        let offset = memory.read_u16(source)?;
        let value = memory.read_u16(at(source, 2)?)?;
        memory.write_u16(0x0400_0000 + u32::from(offset), value)?;
    }
    Ok(())
}

/// Payload 0x1718: contiguous source halfwords, 32-tile destination row pitch,
/// u8 dimensions and a wrapping u16 additive tile/palette delta. Widths above
/// 32 deliberately overlap destination rows, including source aliases.
pub fn stamp_map(
    memory: &mut dyn FrameMemory,
    source: u32,
    destination: u32,
    width: u32,
    height: u32,
    delta: u32,
) -> Result<(), Error> {
    let width = u32::from(width as u8);
    let height = u32::from(height as u8);
    for row in 0..height {
        for column in 0..width {
            let value = memory.read_u16(at(source, (row * width + column) * 2)?)?;
            memory.write_u16(
                at(destination, row * 64 + column * 2)?,
                value.wrapping_add(delta as u16),
            )?;
        }
    }
    Ok(())
}

fn stamp_resource(
    memory: &mut dyn FrameMemory,
    resources: &dyn Resources,
    source: u32,
    destination: u32,
    width: usize,
    height: usize,
    delta: u16,
) -> Result<(), Error> {
    let length = width * height * 2;
    let bytes = exact(resources.read_bytes(source, length)?, source, length)?;
    for row in 0..height {
        for column in 0..width {
            let index = (row * width + column) * 2;
            let value = u16::from_le_bytes([bytes[index], bytes[index + 1]]);
            memory.write_u16(
                at(destination, (row * 64 + column * 2) as u32)?,
                value.wrapping_add(delta),
            )?;
        }
    }
    Ok(())
}
fn upload(memory: &mut dyn FrameMemory, address: u32, bytes: &[u8]) -> Result<(), Error> {
    // All original scene copies have even lengths and aligned halfword lanes.
    // This represents decoded VRAM/palette bytes, not hardware byte-store rules.
    for (index, pair) in bytes.as_chunks::<2>().0.iter().enumerate() {
        memory.write_u16(
            at(address, (index * 2) as u32)?,
            u16::from_le_bytes([pair[0], pair[1]]),
        )?;
    }
    Ok(())
}
fn palette(
    memory: &mut dyn FrameMemory,
    resources: &dyn Resources,
    source: u32,
    destination: u32,
    length: usize,
) -> Result<(), Error> {
    let bytes = exact(resources.read_bytes(source, length)?, source, length)?;
    upload(memory, destination, &bytes)
}
fn tiles(
    memory: &mut dyn FrameMemory,
    resources: &mut dyn Resources,
    source: u32,
    destination: u32,
    length: usize,
) -> Result<(), Error> {
    let bytes = exact(resources.decode_lz77(source)?, source, length)?;
    upload(memory, destination, &bytes)
}

/// Original payload 0xd294..0xd504. Static BG/OBJ memory setup is ported;
/// object initialization, dependency 0x10698(4), and 0xd504 remain explicit
/// Garden calls. Failure preserves native mutations already performed.
pub fn initialize(
    memory: &mut dyn FrameMemory,
    resources: &mut dyn Resources,
    calls: &mut dyn Calls,
) -> Result<(), Error> {
    let registers = exact(resources.read_bytes(0x0202_e074, 64)?, 0x0202_e074, 64)?;
    for pair in registers.as_chunks::<4>().0 {
        let offset = u16::from_le_bytes([pair[0], pair[1]]);
        let value = u16::from_le_bytes([pair[2], pair[3]]);
        memory.write_u16(0x0400_0000 + u32::from(offset), value)?;
    }
    memory.write_u8(GARDEN_FLAG, u8::from(memory.read_u8(SAVED_FLAG)? != 0))?;
    memory.write_u16(GARDEN_FLAG + 2, 0)?;
    memory.write_u8(GARDEN_FLAG + 1, 0)?;
    memory.write_u16(GARDEN_FLAG + 4, 1)?;
    for index in 0..132 {
        memory.write_u8(0x0300_1b70 + index, 0)?;
    }
    // CpuFastSet fill control 0x01000200: 0x200 words, not halfwords.
    for index in 0..0x400 {
        memory.write_u16(0x0600_d000 + index * 2, 0)?;
    }
    palette(memory, resources, 0x0201_7074, 0x0500_0000, 0x1a0)?;
    tiles(memory, resources, 0x0201_7214, 0x0600_0040, 17760)?;
    stamp_resource(memory, resources, 0x0202_dd64, 0x0600_d030, 6, 5, 0x61b6)?;
    stamp_resource(memory, resources, 0x0202_ddb8, 0x0600_d172, 3, 9, 0x71d6)?;
    stamp_resource(memory, resources, 0x0202_ddee, 0x0600_d430, 6, 4, 0x61e1)?;
    if memory.read_u8(GARDEN_FLAG)? != 0 {
        stamp_resource(memory, resources, 0x0202_dda0, 0x0600_d030, 6, 2, 0x61b6)?;
    }
    for row in 0..20 {
        memory.write_u16(0x0600_d002 + row * 64, 0x81f8)?;
        memory.write_u16(0x0600_d02c + row * 64, 0x81f9)?;
    }
    for row in 0..16 {
        let pair = if row & 1 == 0 {
            [0x81fa, 0x81fb]
        } else {
            [0x81fc, 0x81fd]
        };
        for column in 0..20 {
            memory.write_u16(
                0x0600_d004 + row * 64 + column * 2,
                pair[column as usize & 1],
            )?;
        }
    }
    for (row, first, following) in [
        (16, 0x81fe, 0x81ff),
        (17, 0x8200, 0x8201),
        (18, 0x8202, 0x8203),
        (19, 0x8204, 0x8204),
    ] {
        for column in 0..20 {
            memory.write_u16(
                0x0600_d004 + row * 64 + column * 2,
                if column == 0 { first } else { following },
            )?;
        }
    }
    stamp_resource(memory, resources, 0x0202_de1e, 0x0600_d800, 2, 20, 1)?;
    stamp_resource(memory, resources, 0x0202_de6e, 0x0600_d82c, 8, 20, 0x27)?;
    palette(memory, resources, 0x0201_606c, 0x0500_0220, 0xe0)?;
    tiles(memory, resources, 0x0201_614c, 0x0601_0000, 10848)?;
    calls.call(memory, 0x0db0, &[])?;
    calls.call(memory, 0x10698, &[4])?;
    calls.call(memory, 0xd504, &[])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    #[test]
    fn nested_failures_keep_original_dependency_and_memory_addresses() {
        for original in [
            FrameError::Dependency {
                call: NativeCall {
                    entry_offset: 0x02010698,
                    argument0: Some(4),
                },
                reason: "test strict song boundary".into(),
            },
            FrameError::MissingRead {
                address: 0x0203ffff,
                width: 4,
            },
            FrameError::MissingWrite {
                address: 0x06018000,
                width: 2,
            },
        ] {
            assert_eq!(
                frame_error(0xd294, Error::Memory(original.clone())),
                original
            );
        }
    }

    #[derive(Clone)]
    struct Region {
        base: u32,
        bytes: Vec<u8>,
    }
    #[derive(Clone, Default)]
    struct Memory {
        regions: Vec<Region>,
    }
    fn pattern(seed: u32, length: usize) -> Vec<u8> {
        (0..length)
            .map(|index| {
                seed.wrapping_add((index as u32 / 4).wrapping_mul(0x0102_0304))
                    .to_le_bytes()[index % 4]
            })
            .collect()
    }
    fn register_table(seed: u32, count: usize) -> Vec<u8> {
        (0..count)
            .flat_map(|index| {
                let mut pair = [0; 4];
                pair[..2].copy_from_slice(&((index as u16 * 0x12) & 0xfffe).to_le_bytes());
                pair[2..].copy_from_slice(
                    &(seed as u16)
                        .wrapping_add((index as u16).wrapping_mul(0x123))
                        .to_le_bytes(),
                );
                pair
            })
            .collect()
    }
    impl Memory {
        fn bytes(&self, address: u32, length: usize) -> Option<&[u8]> {
            self.regions.iter().find_map(|r| {
                r.bytes
                    .get(usize::try_from(address.checked_sub(r.base)?).ok()?..)?
                    .get(..length)
            })
        }
        fn set(&mut self, address: u32, bytes: &[u8]) -> bool {
            for r in &mut self.regions {
                let Some(index) = address.checked_sub(r.base).map(|i| i as usize) else {
                    continue;
                };
                if let Some(destination) = r
                    .bytes
                    .get_mut(index..)
                    .and_then(|tail| tail.get_mut(..bytes.len()))
                {
                    destination.copy_from_slice(bytes);
                    return true;
                }
            }
            false
        }
        fn new(seed: u32) -> Self {
            let mut memory = Self::default();
            for (base, length, salt) in [
                (0x0200_0000, 0x40000, 0),
                (0x0300_0000, 0x8000, 0xa55a_1234),
                (0x0400_0000, 0x10000, 0x0400),
                (0x0500_0000, 0x1000, 0x0500),
                (0x0600_0000, 0x20000, 0x0600),
            ] {
                memory.regions.push(Region {
                    base,
                    bytes: pattern(seed ^ salt, length),
                });
            }
            assert!(memory.set(0x0203_0000, &pattern(seed ^ 0xeabb_ccdd, 0x10000)));
            memory
        }
        fn snapshot(&self) -> Value {
            let mut output = json!({});
            for (name, address, length) in [
                ("sourceSha256", 0x0203_0000, 0x10000),
                ("iwramSha256", 0x0300_0000, 0x7800),
                ("mmioSha256", 0x0400_0000, 0x10000),
                ("paletteSha256", 0x0500_0000, 0x1000),
                ("vramSha256", 0x0600_0000, 0x20000),
            ] {
                output[name] = json!(crate::sha256(self.bytes(address, length).unwrap()));
            }
            output
        }
    }
    impl FrameMemory for Memory {
        fn read_u8(&self, a: u32) -> Result<u8, FrameError> {
            Ok(*self
                .bytes(a, 1)
                .ok_or(FrameError::MissingRead {
                    address: a,
                    width: 1,
                })?
                .first()
                .unwrap())
        }
        fn read_u16(&self, a: u32) -> Result<u16, FrameError> {
            Ok(u16::from_le_bytes(
                self.bytes(a, 2)
                    .ok_or(FrameError::MissingRead {
                        address: a,
                        width: 2,
                    })?
                    .try_into()
                    .unwrap(),
            ))
        }
        fn read_u32(&self, a: u32) -> Result<u32, FrameError> {
            Ok(u32::from_le_bytes(
                self.bytes(a, 4)
                    .ok_or(FrameError::MissingRead {
                        address: a,
                        width: 4,
                    })?
                    .try_into()
                    .unwrap(),
            ))
        }
        fn write_u8(&mut self, a: u32, v: u8) -> Result<(), FrameError> {
            if self.set(a, &[v]) {
                Ok(())
            } else {
                Err(FrameError::MissingWrite {
                    address: a,
                    width: 1,
                })
            }
        }
        fn write_u16(&mut self, a: u32, v: u16) -> Result<(), FrameError> {
            if self.set(a, &v.to_le_bytes()) {
                Ok(())
            } else {
                Err(FrameError::MissingWrite {
                    address: a,
                    width: 2,
                })
            }
        }
    }
    struct SyntheticResources {
        seed: u32,
        short: bool,
    }
    impl Resources for SyntheticResources {
        fn read_bytes(&self, address: u32, length: usize) -> Result<Vec<u8>, Error> {
            let mut bytes = if address == 0x0202_e074 {
                register_table(self.seed, 16)
            } else {
                pattern(self.seed ^ address, length)
            };
            if self.short {
                bytes.pop();
            }
            Ok(bytes)
        }
        fn decode_lz77(&mut self, address: u32) -> Result<Vec<u8>, Error> {
            let length = match address {
                0x0201_7214 => 17760,
                0x0201_614c => 10848,
                _ => panic!("unknown synthetic codec boundary"),
            };
            Ok(pattern(
                self.seed ^ address,
                length - usize::from(self.short),
            ))
        }
    }
    #[derive(Default)]
    struct RecordingCalls {
        calls: Vec<Value>,
    }
    impl Calls for RecordingCalls {
        fn call(
            &mut self,
            _: &mut dyn FrameMemory,
            offset: u32,
            args: &[u32],
        ) -> Result<(), FrameError> {
            self.calls
                .push(json!({"offset": offset, "arguments": args}));
            Ok(()) // Explicit test-only boundary. Production rejects these.
        }
    }
    fn fixture() -> Value {
        serde_json::from_str(vectors!("garden-graphics-vectors.json")).unwrap()
    }
    fn word(case: &Value, name: &str) -> u32 {
        case[name].as_u64().unwrap() as u32
    }
    fn check_case(case: &Value, resources: &mut dyn Resources) {
        let seed = word(case, "seed");
        let mut memory = Memory::new(seed);
        memory
            .write_u8(SAVED_FLAG, case["gardenFlag"].as_u64().unwrap_or(0) as u8)
            .unwrap();
        let mut calls = RecordingCalls::default();
        match case["routine"].as_str().unwrap() {
            "registerScript" => {
                assert!(memory.set(0x0203_0000, &register_table(seed, 255)));
                apply_register_script(&mut memory, word(case, "source"), word(case, "count"))
                    .unwrap();
            }
            "mapStamp" => stamp_map(
                &mut memory,
                word(case, "source"),
                word(case, "destination"),
                word(case, "width"),
                word(case, "height"),
                word(case, "delta"),
            )
            .unwrap(),
            "initialize" => initialize(&mut memory, resources, &mut calls).unwrap(),
            _ => panic!("unknown native Garden case"),
        }
        assert_eq!(json!(calls.calls), case["calls"], "native calls: {case}");
        for (name, expected) in memory.snapshot().as_object().unwrap() {
            assert_eq!(expected, &case[name], "{name}: {case}");
        }
    }

    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn independent_native_helpers_and_static_scene() {
        let data = fixture();
        assert_eq!(data["space"], "garden");
        assert_eq!(data["payloadSha256"], PAYLOAD_SHA256);
        assert_eq!(data["payloadLength"], PAYLOAD_LENGTH);
        assert_eq!(data["cases"].as_array().unwrap().len(), 163);
        for case in data["cases"].as_array().unwrap() {
            check_case(
                case,
                &mut SyntheticResources {
                    seed: word(case, "seed"),
                    short: false,
                },
            );
        }
    }
    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn native_source_and_bios_metadata_are_exact() {
        let data = fixture();
        for (name, offset, length, hash) in SOURCE_SPANS {
            let source = data["routines"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["name"] == name)
                .unwrap();
            assert_eq!(
                source,
                &json!({"name": name, "offset": offset, "length": length, "sha256": hash})
            );
        }
        for (name, offset, length, hash) in RESOURCE_SPANS {
            let resource = data["resources"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["name"] == name)
                .unwrap();
            assert_eq!(resource["offset"], offset);
            assert_eq!(resource["length"], length);
            assert_eq!(resource["sha256"], hash);
        }
        let bios = json!([
            {"kind":"cpuFastSet","destination":0x0600_d000u32,"control":0x0100_0200u32,"fillWord":0},
            {"kind":"cpuFastSet","source":0x0201_7074u32,"destination":0x0500_0000u32,"control":0x68},
            {"kind":"lz77Vram","source":0x0201_7214u32,"destination":0x0600_0040u32,"decodedLength":17760},
            {"kind":"cpuFastSet","source":0x0201_606cu32,"destination":0x0500_0220u32,"control":0x38},
            {"kind":"lz77Vram","source":0x0201_614cu32,"destination":0x0601_0000u32,"decodedLength":10848}
        ]);
        for case in data["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["routine"] == "initialize")
        {
            assert_eq!(case["bios"], bios);
        }
    }
    #[test]
    fn zero_dimensions_truncation_and_unbound_memory_stay_bounded() {
        let mut memory = Memory::default();
        apply_register_script(&mut memory, u32::MAX, 256).unwrap();
        stamp_map(&mut memory, u32::MAX, u32::MAX, 0, 255, 0).unwrap();
        stamp_map(&mut memory, u32::MAX, u32::MAX, 255, 256, 0).unwrap();
        assert!(matches!(
            apply_register_script(&mut memory, 0x0203_0000, 1),
            Err(Error::Memory(FrameError::MissingRead { .. }))
        ));
        memory.regions.push(Region {
            base: 0xffff_fffe,
            bytes: vec![0xff, 0xff],
        });
        memory.regions.push(Region {
            base: 0x0600_0000,
            bytes: vec![0; 4],
        });
        assert!(matches!(
            stamp_map(&mut memory, 0xffff_fffe, 0x0600_0000, 2, 1, 1),
            Err(Error::AddressOverflow { .. })
        ));
        assert_eq!(memory.read_u16(0x0600_0000).unwrap(), 0);
        assert!(matches!(
            stamp_map(&mut memory, 0xffff_fffe, 0x0600_0004, 1, 1, 0),
            Err(Error::Memory(FrameError::MissingWrite { .. }))
        ));
    }
    #[test]
    fn strict_dependencies_and_resource_lengths_fail_after_native_prefix() {
        let mut memory = Memory::new(7);
        let mut resources = SyntheticResources {
            seed: 7,
            short: false,
        };
        assert!(matches!(
            initialize(&mut memory, &mut resources, &mut RejectCalls),
            Err(Error::Memory(FrameError::Dependency {
                call: NativeCall {
                    entry_offset: 0x0db0,
                    ..
                },
                ..
            }))
        ));
        assert_eq!(memory.read_u16(0x0300_1ad4).unwrap(), 1);
        assert_eq!(memory.read_u16(0x0600_d004).unwrap(), 0x81fa);
        let mut memory = Memory::new(7);
        let before = memory.snapshot();
        resources.short = true;
        assert!(matches!(
            initialize(&mut memory, &mut resources, &mut RejectCalls),
            Err(Error::ResourceLength {
                address: 0x0202_e074,
                expected: 64,
                actual: 63
            })
        ));
        assert_eq!(memory.snapshot(), before);
        assert!(matches!(
            tiles(&mut memory, &mut resources, 0x0201_7214, 0x0600_0040, 17760),
            Err(Error::ResourceLength { .. })
        ));
        assert!(GardenPayload::new(&[]).is_err());
        assert!(GardenPayload::new(&vec![0; PAYLOAD_LENGTH]).is_err());
    }
    #[cfg_attr(not(spp_vectors), ignore = "needs local Pinball Party vectors")]
    #[test]
    fn authentic_local_payload_resources_and_native_scene() {
        let Ok(path) = std::env::var("SPP_GARDEN_PAYLOAD") else {
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        let mut resources = GardenPayload::new(&bytes).unwrap();
        for (_, offset, length, hash) in SOURCE_SPANS.into_iter().chain(RESOURCE_SPANS) {
            assert_eq!(crate::sha256(&bytes[offset..offset + length]), hash);
        }
        for (address, length, hash) in DECODED_TILES {
            let decoded = resources.decode_lz77(address).unwrap();
            assert_eq!(decoded.len(), length);
            assert_eq!(crate::sha256(&decoded), hash);
            let item = fixture()["resources"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["offset"] == address - PAYLOAD_BASE)
                .cloned()
                .unwrap();
            assert_eq!(item["decodedLength"], length);
            assert_eq!(item["decodedSha256"], hash);
        }
        for case in fixture()["authenticCases"].as_array().unwrap() {
            check_case(case, &mut resources);
        }
        assert!(resources.read_bytes(PAYLOAD_BASE - 1, 1).is_err());
        assert!(resources
            .read_bytes(PAYLOAD_BASE + PAYLOAD_LENGTH as u32 - 1, 2)
            .is_err());
        assert!(resources
            .read_bytes(PAYLOAD_BASE, MAX_RESOURCE_BYTES + 1)
            .is_err());
        assert!(resources
            .decode_lz77(PAYLOAD_BASE + PAYLOAD_LENGTH as u32)
            .is_err());
        assert!(resources.decode_lz77(0x0801_7214).is_err());
        let mut changed = bytes;
        changed[0x17214] ^= 1;
        assert!(GardenPayload::new(&changed).is_err());
    }
}
