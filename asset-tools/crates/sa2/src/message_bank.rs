//! Localized PRS message banks with big-endian string offsets and a -1 sentinel.
use crate::{archive, manifest, Error, Result};
use serde_json::{json, Value};

pub fn read_prs(bytes: &[u8]) -> Result<Value> {
    let data = archive::decompress_prs(bytes, 4 * 1024 * 1024)?;
    read(&data)
}
fn read(data: &[u8]) -> Result<Value> {
    let invalid = || Error::invalid("invalid SA2 message offset table or string");
    let word = |off| -> Result<u32> {
        Ok(u32::from_be_bytes(
            data.get(off..off + 4)
                .ok_or_else(invalid)?
                .try_into()
                .unwrap(),
        ))
    };
    let first = word(0)? as usize;
    if first < 8 || first > data.len() || !first.is_multiple_of(4) || word(first - 4)? != u32::MAX {
        return Err(invalid());
    }
    let count = first / 4 - 1;
    if count > 65536 {
        return Err(invalid());
    }
    let mut rows = Vec::with_capacity(count);
    for group in 0..count {
        let start = word(4 * group)? as usize;
        let end = if group + 1 == count {
            data.len()
        } else {
            word(4 * group + 4)? as usize
        };
        let slice = data
            .get(start..end)
            .filter(|_| start >= first)
            .ok_or_else(invalid)?;
        // Some numbered banks store a line feed after the string terminator.
        let (payload, trailer) = match slice.strip_suffix(&[0, 10]) {
            Some(payload) => (payload, &slice[slice.len() - 1..]),
            None => (
                slice.strip_suffix(&[0]).ok_or_else(invalid)?,
                &slice[slice.len()..],
            ),
        };
        if payload.contains(&0) {
            return Err(invalid());
        }
        rows.push(json!({"group":group,"offset":start,"payload":payload,"postTerminatorBytes":trailer,"sha256":manifest::sha256(payload)}));
    }
    Ok(json!({"decodedSha256":manifest::sha256(data),"groups":rows}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn separates_line_feed_after_nul_from_message_content() {
        let bank = read(&[
            0, 0, 0, 12, 0, 0, 0, 16, 255, 255, 255, 255, 65, 10, 0, 10, 66, 0,
        ])
        .unwrap();
        let rows = &bank["groups"];
        assert_eq!(rows[0]["payload"], json!([65, 10]));
        assert_eq!(rows[0]["postTerminatorBytes"], json!([10]));
        assert_eq!(rows[1]["payload"], json!([66]));
        assert_eq!(rows[1]["postTerminatorBytes"], json!([]));
    }
    #[test]
    fn rejects_bad_sentinel_offsets_and_unterminated_strings() {
        for data in [
            &[0, 0, 0, 8, 0, 0, 0, 0, 65, 0][..],
            &[0, 0, 0, 8, 255, 255, 255, 255, 65],
            &[0, 0, 0, 12, 0, 0, 0, 8, 255, 255, 255, 255, 65, 0],
        ] {
            assert!(read(data).is_err());
        }
    }
}
