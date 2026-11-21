//! A whole memory card as one blob, the way the server keeps it: `SA2CARD1`
//! then, per file: name length (u8), name, status length (u8), the status
//! bytes `CARDSetStatus` stored, data length (u32 LE), data.

const MAGIC: &[u8] = b"SA2CARD1";

/// A card file as (name, status bytes, data).
pub type CardFile = (String, Vec<u8>, Vec<u8>);

pub fn pack(files: &[CardFile]) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    for (name, stat, data) in files {
        out.push(name.len() as u8);
        out.extend_from_slice(name.as_bytes());
        out.push(stat.len() as u8);
        out.extend_from_slice(stat);
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
    }
    out
}

pub fn unpack(bytes: &[u8]) -> Option<Vec<CardFile>> {
    let mut rest = bytes.strip_prefix(MAGIC)?;
    let mut take = |n: usize| {
        let (head, tail) = (rest.get(..n)?, rest.get(n..)?);
        rest = tail;
        Some(head.to_vec())
    };
    let mut files = Vec::new();
    while let Some(len) = take(1) {
        let name = String::from_utf8(take(len[0] as usize)?).ok()?;
        let stat_len = take(1)?[0] as usize;
        let stat = take(stat_len)?;
        let size = u32::from_le_bytes(take(4)?.try_into().ok()?) as usize;
        files.push((name, stat, take(size)?));
    }
    Some(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_survives_the_trip() {
        let files = vec![
            ("SONIC2B__ALF".to_owned(), vec![1; 24], vec![7; 65536]),
            ("SONIC2B__S01".to_owned(), Vec::new(), vec![]),
        ];
        assert_eq!(unpack(&pack(&files)), Some(files.clone()));
        assert_eq!(unpack(&pack(&[])), Some(vec![]));
        let whole = pack(&files);
        assert_eq!(
            unpack(&whole[..whole.len() - 1]),
            None,
            "a cut card is refused"
        );
        assert_eq!(unpack(b"not a card"), None);
    }
}
