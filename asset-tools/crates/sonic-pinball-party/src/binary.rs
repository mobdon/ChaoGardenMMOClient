//! Bounded GBA BIOS stream readers; offsets always refer to the supplied slice.
use std::fmt;

pub const MAX_DECODED: usize = 0x40000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Truncated(usize),
    InvalidTag(u8),
    OutputLimit { declared: usize, limit: usize },
    InvalidBackReference { at: usize, distance: usize },
    OutputOverrun,
    Range,
    Allocation,
    Identity,
    HashMismatch(String),
    ScanBudget,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

pub fn span(bytes: &[u8], offset: usize, length: usize) -> Result<&[u8], Error> {
    bytes
        .get(offset..offset.checked_add(length).ok_or(Error::Range)?)
        .ok_or(Error::Range)
}

pub fn u16_le(bytes: &[u8], offset: usize) -> Result<u16, Error> {
    Ok(u16::from_le_bytes(
        span(bytes, offset, 2)?
            .try_into()
            .map_err(|_| Error::Range)?,
    ))
}

pub fn u32_le(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(
        span(bytes, offset, 4)?
            .try_into()
            .map_err(|_| Error::Range)?,
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub bytes: Vec<u8>,
    /// Token bytes consumed, excluding alignment padding and following resources.
    pub consumed: usize,
}

pub fn declared_size(bytes: &[u8]) -> Result<usize, Error> {
    let h = bytes.get(..4).ok_or(Error::Truncated(bytes.len()))?;
    if !matches!(h[0], 0x10 | 0x30) {
        return Err(Error::InvalidTag(h[0]));
    }
    Ok(usize::from(h[1]) | usize::from(h[2]) << 8 | usize::from(h[3]) << 16)
}

/// Type 0x10 LZ77 or type 0x30 run length. Strictly rejects a final token
/// exceeding the declared output. Does not emulate BIOS memory corruption.
pub fn decode(bytes: &[u8], max_output: usize) -> Result<Decoded, Error> {
    let length = declared_size(bytes)?;
    let limit = max_output.min(MAX_DECODED);
    if length > limit {
        return Err(Error::OutputLimit {
            declared: length,
            limit,
        });
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(length)
        .map_err(|_| Error::Allocation)?;
    let mut cursor = 4;
    while output.len() < length {
        let flags = *bytes.get(cursor).ok_or(Error::Truncated(cursor))?;
        cursor += 1;
        if bytes[0] == 0x30 {
            let count = usize::from(flags & 0x7f) + if flags & 0x80 != 0 { 3 } else { 1 };
            if count > length - output.len() {
                return Err(Error::OutputOverrun);
            }
            if flags & 0x80 != 0 {
                let value = *bytes.get(cursor).ok_or(Error::Truncated(cursor))?;
                cursor += 1;
                output.resize(output.len() + count, value);
            } else {
                let values = span(bytes, cursor, count).map_err(|_| Error::Truncated(cursor))?;
                output.extend_from_slice(values);
                cursor += count;
            }
        } else {
            for bit in (0..8).rev() {
                if output.len() == length {
                    break;
                }
                if flags & (1 << bit) == 0 {
                    output.push(*bytes.get(cursor).ok_or(Error::Truncated(cursor))?);
                    cursor += 1;
                } else {
                    let pair = span(bytes, cursor, 2).map_err(|_| Error::Truncated(cursor))?;
                    cursor += 2;
                    let distance = ((usize::from(pair[0]) & 15) << 8 | usize::from(pair[1])) + 1;
                    let count = usize::from(pair[0] >> 4) + 3;
                    if distance > output.len() {
                        return Err(Error::InvalidBackReference {
                            at: output.len(),
                            distance,
                        });
                    }
                    if count > length - output.len() {
                        return Err(Error::OutputOverrun);
                    }
                    for _ in 0..count {
                        output.push(output[output.len() - distance]);
                    }
                }
            }
        }
    }
    Ok(Decoded {
        bytes: output,
        consumed: cursor,
    })
}

/// Convert a pointer only within its explicitly declared address space.
pub fn pointer_offset(pointer: u32, base: u32, length: usize) -> Result<usize, Error> {
    let offset = usize::try_from(pointer.checked_sub(base).ok_or(Error::Range)?)
        .map_err(|_| Error::Range)?;
    if offset >= length {
        return Err(Error::Range);
    }
    Ok(offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlap_and_consumed_exclude_padding() {
        let d = decode(&[0x10, 6, 0, 0, 0x40, b'A', 0x20, 0, 0xff], 6).unwrap();
        assert_eq!(d.bytes, b"AAAAAA");
        assert_eq!(d.consumed, 8);
    }
    #[test]
    fn rle_mixed_packets() {
        let d = decode(&[0x30, 6, 0, 0, 1, 1, 2, 0x81, 3, 0xff], 6).unwrap();
        assert_eq!(d.bytes, [1, 2, 3, 3, 3, 3]);
        assert_eq!(d.consumed, 9);
    }
    #[test]
    fn malformed_and_limits() {
        for data in [
            &[][..],
            &[0x10, 1],
            &[0x10, 1, 0, 0],
            &[0x10, 1, 0, 0, 0],
            &[0x30, 4, 0, 0, 0x81],
        ] {
            assert!(decode(data, 4).is_err());
        }
        assert!(matches!(
            decode(&[0x10, 3, 0, 0, 0x80, 0, 0], 3),
            Err(Error::InvalidBackReference { .. })
        ));
        assert_eq!(
            decode(&[0x30, 1, 0, 0, 0x80, 0], 1),
            Err(Error::OutputOverrun)
        );
        assert!(matches!(
            decode(&[0x10, 0xff, 0xff, 0xff], usize::MAX),
            Err(Error::OutputLimit { .. })
        ));
        assert_eq!(span(&[1], usize::MAX, 2), Err(Error::Range));
        assert!(pointer_offset(0x02000000, 0x08000000, 8).is_err());
        assert!(pointer_offset(0x08000008, 0x08000000, 8).is_err());
    }
    #[test]
    fn arbitrary_short_inputs_never_panic() {
        for tag in 0..=255 {
            for n in 0..=32 {
                let input = vec![tag; n];
                let _ = decode(&input, 32);
            }
        }
    }
}
