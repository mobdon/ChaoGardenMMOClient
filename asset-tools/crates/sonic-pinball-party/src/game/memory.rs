//! One bounded address image for source-port composition. Every RAM/register
//! byte is supplied explicitly; no unbound read becomes zero. This is snapshot
//! storage, not hardware: no DMA, VRAM mirrors, MMIO side effects or CPU execute.
use super::{display, frame, intro, sound_dma, startup};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryError {
    SourceIdentity,
    RegionLimit,
    InvalidRegion(u32),
    Overlap(u32),
    Missing {
        address: u32,
        length: usize,
        write: bool,
    },
    ReadOnly(u32),
}
impl std::fmt::Display for MemoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for MemoryError {}

pub struct Region {
    pub base: u32,
    pub bytes: Vec<u8>,
    pub writable: bool,
}
pub struct MemoryImage<'a> {
    rom: &'a [u8],
    regions: Vec<Region>,
}
impl<'a> MemoryImage<'a> {
    #[cfg(test)]
    pub(super) fn fixture(regions: Vec<Region>) -> MemoryImage<'static> {
        MemoryImage::bind(&[], regions).expect("bounded explicit test regions")
    }
    /// Authenticate the exact local cartridge and bind the caller's explicit
    /// snapshots. The image never initializes machine RAM or claims a reset.
    pub fn new(rom: &'a [u8], regions: Vec<Region>) -> Result<Self, MemoryError> {
        crate::catalog::validate_rom(rom).map_err(|_| MemoryError::SourceIdentity)?;
        Self::bind(rom, regions)
    }
    fn bind(rom: &'a [u8], mut regions: Vec<Region>) -> Result<Self, MemoryError> {
        if regions.len() > 64
            || regions
                .iter()
                .try_fold(0usize, |n, r| n.checked_add(r.bytes.len()))
                .is_none_or(|n| n > 0x200000)
        {
            return Err(MemoryError::RegionLimit);
        }
        regions.sort_by_key(|r| r.base);
        let mut previous_end = 0;
        for r in &regions {
            let length =
                u32::try_from(r.bytes.len()).map_err(|_| MemoryError::InvalidRegion(r.base))?;
            let end = r
                .base
                .checked_add(length)
                .ok_or(MemoryError::InvalidRegion(r.base))?;
            if length == 0 {
                return Err(MemoryError::InvalidRegion(r.base));
            }
            if r.base < previous_end || (r.base < 0x08800000 && end > 0x08000000) {
                return Err(MemoryError::Overlap(r.base));
            }
            previous_end = end;
        }
        Ok(Self { rom, regions })
    }
    pub fn bytes(&self, address: u32, length: usize) -> Result<&[u8], MemoryError> {
        let missing = || MemoryError::Missing {
            address,
            length,
            write: false,
        };
        let end = u32::try_from(length)
            .ok()
            .and_then(|n| address.checked_add(n))
            .ok_or_else(missing)?;
        if address >= 0x08000000 && end <= 0x08800000 {
            let start = (address - 0x08000000) as usize;
            return self.rom.get(start..start + length).ok_or_else(missing);
        }
        let index = self
            .regions
            .partition_point(|r| r.base <= address)
            .checked_sub(1)
            .ok_or_else(missing)?;
        let region = &self.regions[index];
        let start = (address - region.base) as usize;
        region.bytes.get(start..start + length).ok_or_else(missing)
    }
    /// Validate the entire destination before the first write. Adjacent region
    /// boundaries are not implicitly merged; mirrors require an explicit host.
    pub fn write_bytes(&mut self, address: u32, bytes: &[u8]) -> Result<(), MemoryError> {
        let missing = || MemoryError::Missing {
            address,
            length: bytes.len(),
            write: true,
        };
        u32::try_from(bytes.len())
            .ok()
            .and_then(|n| address.checked_add(n))
            .ok_or_else(missing)?;
        if (0x08000000..0x08800000).contains(&address) {
            return Err(MemoryError::ReadOnly(address));
        }
        let index = self
            .regions
            .partition_point(|r| r.base <= address)
            .checked_sub(1)
            .ok_or_else(missing)?;
        let region = &mut self.regions[index];
        let start = (address - region.base) as usize;
        if !region.writable {
            return Err(MemoryError::ReadOnly(address));
        }
        region
            .bytes
            .get_mut(start..start + bytes.len())
            .ok_or_else(missing)?
            .copy_from_slice(bytes);
        Ok(())
    }
    fn read<const N: usize>(&self, address: u32) -> Result<[u8; N], MemoryError> {
        Ok(self
            .bytes(address, N)?
            .try_into()
            .expect("exact requested span"))
    }
}
fn read_error(address: u32, width: u8) -> frame::FrameError {
    frame::FrameError::MissingRead { address, width }
}
fn write_error(address: u32, width: u8) -> frame::FrameError {
    frame::FrameError::MissingWrite { address, width }
}
impl frame::FrameMemory for MemoryImage<'_> {
    fn read_u8(&self, a: u32) -> Result<u8, frame::FrameError> {
        self.read::<1>(a)
            .map(|v| v[0])
            .map_err(|_| read_error(a, 1))
    }
    fn read_u16(&self, a: u32) -> Result<u16, frame::FrameError> {
        self.read(a)
            .map(u16::from_le_bytes)
            .map_err(|_| read_error(a, 2))
    }
    fn read_u32(&self, a: u32) -> Result<u32, frame::FrameError> {
        self.read(a)
            .map(u32::from_le_bytes)
            .map_err(|_| read_error(a, 4))
    }
    fn write_u8(&mut self, a: u32, v: u8) -> Result<(), frame::FrameError> {
        self.write_bytes(a, &[v]).map_err(|_| write_error(a, 1))
    }
    fn write_u16(&mut self, a: u32, v: u16) -> Result<(), frame::FrameError> {
        self.write_bytes(a, &v.to_le_bytes())
            .map_err(|_| write_error(a, 2))
    }
    fn write_bytes(&mut self, a: u32, bytes: &[u8]) -> Result<(), frame::FrameError> {
        MemoryImage::write_bytes(self, a, bytes).map_err(|_| write_error(a, 1))
    }
}
impl intro::IntroMemory for MemoryImage<'_> {
    fn read_halfword(&self, a: u32) -> Result<u16, intro::IntroError> {
        self.read(a)
            .map(u16::from_le_bytes)
            .map_err(|_| intro::IntroError::Memory {
                address: a,
                length: 2,
            })
    }
    fn read_word(&self, a: u32) -> Result<u32, intro::IntroError> {
        self.read(a)
            .map(u32::from_le_bytes)
            .map_err(|_| intro::IntroError::Memory {
                address: a,
                length: 4,
            })
    }
    fn write_halfword(&mut self, a: u32, v: u16) -> Result<(), intro::IntroError> {
        MemoryImage::write_bytes(self, a, &v.to_le_bytes()).map_err(|_| intro::IntroError::Memory {
            address: a,
            length: 2,
        })
    }
    fn write_word(&mut self, a: u32, v: u32) -> Result<(), intro::IntroError> {
        MemoryImage::write_bytes(self, a, &v.to_le_bytes()).map_err(|_| intro::IntroError::Memory {
            address: a,
            length: 4,
        })
    }
    fn write_bytes(&mut self, a: u32, v: &[u8]) -> Result<(), intro::IntroError> {
        MemoryImage::write_bytes(self, a, v).map_err(|_| intro::IntroError::Memory {
            address: a,
            length: v.len(),
        })
    }
}
impl startup::StartupMemory for MemoryImage<'_> {
    fn read_word(&self, a: u32) -> Result<u32, startup::StartupError> {
        self.read(a)
            .map(u32::from_le_bytes)
            .map_err(|_| startup::StartupError::MissingWord(a))
    }
    fn write_word(&mut self, a: u32, v: u32) -> Result<(), startup::StartupError> {
        self.write_bytes(a, &v.to_le_bytes())
            .map_err(|_| startup::StartupError::MissingWord(a))
    }
}
impl display::DisplayMemory for MemoryImage<'_> {
    fn read_halfword(&self, a: u32) -> Result<u16, display::DisplayError> {
        self.read(a)
            .map(u16::from_le_bytes)
            .map_err(|_| display::DisplayError::Memory {
                address: a,
                write: false,
            })
    }
    fn write_halfword(&mut self, a: u32, v: u16) -> Result<(), display::DisplayError> {
        self.write_bytes(a, &v.to_le_bytes())
            .map_err(|_| display::DisplayError::Memory {
                address: a,
                write: true,
            })
    }
}
impl sound_dma::SoundDmaMemory for MemoryImage<'_> {
    fn read_word(&self, a: u32) -> Result<u32, sound_dma::SoundDmaError> {
        self.read(a)
            .map(u32::from_le_bytes)
            .map_err(|_| sound_dma::SoundDmaError::Memory {
                address: a,
                width: 4,
                write: false,
            })
    }
    fn write_word(&mut self, a: u32, v: u32) -> Result<(), sound_dma::SoundDmaError> {
        self.write_bytes(a, &v.to_le_bytes())
            .map_err(|_| sound_dma::SoundDmaError::Memory {
                address: a,
                width: 4,
                write: true,
            })
    }
    fn write_halfword(&mut self, a: u32, v: u16) -> Result<(), sound_dma::SoundDmaError> {
        self.write_bytes(a, &v.to_le_bytes())
            .map_err(|_| sound_dma::SoundDmaError::Memory {
                address: a,
                width: 2,
                write: true,
            })
    }
    fn write_byte(&mut self, a: u32, v: u8) -> Result<(), sound_dma::SoundDmaError> {
        self.write_bytes(a, &[v])
            .map_err(|_| sound_dma::SoundDmaError::Memory {
                address: a,
                width: 1,
                write: true,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn region(base: u32, bytes: Vec<u8>, writable: bool) -> Region {
        Region {
            base,
            bytes,
            writable,
        }
    }
    #[test]
    fn rejects_overlaps_wrapping_regions_and_unauthenticated_rom() {
        assert!(matches!(
            MemoryImage::bind(
                &[],
                vec![
                    region(0x2000000, vec![1; 4], true),
                    region(0x2000002, vec![2; 4], true)
                ]
            ),
            Err(MemoryError::Overlap(_))
        ));
        assert!(matches!(
            MemoryImage::bind(&[], vec![region(u32::MAX, vec![1; 2], true)]),
            Err(MemoryError::InvalidRegion(_))
        ));
        assert!(matches!(
            MemoryImage::bind(&[], vec![region(0x8000000, vec![1], true)]),
            Err(MemoryError::Overlap(_))
        ));
        assert!(matches!(
            MemoryImage::new(&[], vec![]),
            Err(MemoryError::SourceIdentity)
        ));
    }
    #[test]
    fn span_errors_are_atomic_and_read_only_sources_stay_unchanged() {
        let mut m = MemoryImage::bind(
            &[9; 8],
            vec![
                region(0x02000000, vec![1, 2, 3, 4], true),
                region(0x03000000, vec![5, 6], false),
            ],
        )
        .unwrap();
        assert!(m.write_bytes(0x02000002, &[8; 4]).is_err());
        assert_eq!(m.bytes(0x02000000, 4).unwrap(), [1, 2, 3, 4]);
        assert!(m.bytes(u32::MAX, 2).is_err());
        assert!(matches!(
            m.write_bytes(0x03000000, &[0]),
            Err(MemoryError::ReadOnly(_))
        ));
        assert!(matches!(
            m.write_bytes(0x08000000, &[0]),
            Err(MemoryError::ReadOnly(_))
        ));
        assert_eq!(m.bytes(0x08000000, 8).unwrap(), [9; 8]);
        assert!(m.bytes(0x08000008, 1).is_err());
    }
    #[test]
    fn all_source_port_adapters_share_the_same_bytes() {
        let a = startup::CONTEXT_POINTER_SLOT;
        let mut m = MemoryImage::bind(&[], vec![region(a, vec![0x55; 8], true)]).unwrap();
        startup::StartupMemory::write_word(&mut m, a, 0x020199a0).unwrap();
        assert_eq!(intro::IntroMemory::read_word(&m, a).unwrap(), 0x020199a0);
        assert_eq!(frame::FrameMemory::read_u32(&m, a).unwrap(), 0x020199a0);
        frame::FrameMemory::write_u8(&mut m, a + 3, 0x12).unwrap();
        assert_eq!(
            sound_dma::SoundDmaMemory::read_word(&m, a).unwrap(),
            0x120199a0
        );
        display::DisplayMemory::write_halfword(&mut m, a + 4, 0xabcd).unwrap();
        assert_eq!(
            intro::IntroMemory::read_halfword(&m, a + 4).unwrap(),
            0xabcd
        );
        assert!(frame::FrameMemory::read_u8(&m, a + 8).is_err());
    }
}
