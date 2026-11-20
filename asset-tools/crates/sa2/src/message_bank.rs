//! Localized PRS message banks with big-endian string offsets, usually ended by
//! a -1 sentinel.
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
    if first < 8 || first > data.len() || !first.is_multiple_of(4) {
        return Err(invalid());
    }
    // Most banks end their offset table with -1. The English, Japanese, German
    // and Spanish in-stage system text banks (text_*.prs, byte-identical on the
    // GameCube disc) end it directly at the first string instead.
    let terminated = word(first - 4)? == u32::MAX;
    let count = first / 4 - usize::from(terminated);
    if count > 65536 {
        return Err(invalid());
    }
    let offsets = (0..count)
        .map(|g| word(4 * g).map(|o| o as usize))
        .collect::<Result<Vec<_>>>()?;
    // Several entries may share one string; each string runs to the next
    // distinct offset (or the end of the bank).
    let mut starts = offsets.clone();
    starts.sort_unstable();
    starts.dedup();
    let mut rows = Vec::with_capacity(count);
    for (group, &start) in offsets.iter().enumerate() {
        let end = starts
            .get(starts.partition_point(|&s| s <= start))
            .copied()
            .unwrap_or(data.len());
        let slice = data
            .get(start..end)
            .filter(|_| start >= first)
            .ok_or_else(invalid)?;
        // Some numbered banks store a line feed after the string terminator;
        // the sentinel-less text banks pad each string with zeros instead.
        let nul = slice.iter().position(|b| *b == 0).ok_or_else(invalid)?;
        let (payload, trailer) = (&slice[..nul], &slice[nul + 1..]);
        let padding = !terminated && trailer.iter().all(|b| *b == 0);
        if !(trailer.is_empty() || trailer == [10] || padding) {
            return Err(invalid());
        }
        rows.push(json!({"group":group,"offset":start,"payload":payload,"postTerminatorBytes":trailer,"sha256":manifest::sha256(payload)}));
    }
    Ok(json!({"decodedSha256":manifest::sha256(data),"terminated":terminated,"groups":rows}))
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
    #[test]
    fn reads_tables_without_sentinel_and_shared_strings() {
        // Three entries, no -1: the first two share "A", the third is "B".
        let bank = read(&[0, 0, 0, 12, 0, 0, 0, 12, 0, 0, 0, 14, 65, 0, 66, 0]).unwrap();
        assert_eq!(bank["terminated"], json!(false));
        let rows = bank["groups"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["payload"], json!([65]));
        assert_eq!(rows[1]["payload"], json!([65]));
        assert_eq!(rows[2]["payload"], json!([66]));
        // Out-of-order entries and zero padding to four bytes, as in text_e.prs.
        let padded = read(&[
            0, 0, 0, 12, 0, 0, 0, 16, 0, 0, 0, 12, 65, 66, 0, 0, 67, 0, 0, 0,
        ])
        .unwrap();
        let rows = padded["groups"].as_array().unwrap();
        assert_eq!(rows[0]["payload"], json!([65, 66]));
        assert_eq!(rows[0]["postTerminatorBytes"], json!([0]));
        assert_eq!(rows[1]["payload"], json!([67]));
        assert_eq!(rows[1]["postTerminatorBytes"], json!([0, 0]));
        assert_eq!(rows[2]["payload"], json!([65, 66]));
        // Zero padding is only accepted where there is no sentinel, and other
        // data after a terminator is never padding.
        assert!(read(&[0, 0, 0, 8, 255, 255, 255, 255, 65, 0, 0, 0]).is_err());
        assert!(read(&[0, 0, 0, 8, 0, 0, 0, 8, 65, 0, 66, 0]).is_err());
        // The sentinel form is unchanged.
        let sentinel = read(&[0, 0, 0, 8, 255, 255, 255, 255, 65, 0]).unwrap();
        assert_eq!(sentinel["terminated"], json!(true));
        assert_eq!(sentinel["groups"].as_array().unwrap().len(), 1);
    }
}
