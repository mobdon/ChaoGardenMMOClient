//! Offline interpretation of the PC SA2 StoryEntry arrays.
use crate::{object_table::verify_image, pe::PeImage, stage_catalog::sections, Error, Result};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[repr(i8)]
#[serde(rename_all = "camelCase")]
pub enum EntryType {
    Event = 0,
    Level = 1,
    End = 2,
    Credits = 3,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub rva: u32,
    pub kind: EntryType,
    pub character: i8,
    pub level: i16,
    pub event_words: [i16; 4],
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sequence {
    pub start_rva: u32,
    pub source_filename: String,
    entries: Vec<Entry>,
    pub terminator: Entry,
}
impl Sequence {
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
}

fn decode(bytes: &[u8; 12], rva: u32) -> Result<Entry> {
    let word = |pos| i16::from_le_bytes([bytes[pos], bytes[pos + 1]]);
    let kind = match bytes[0] {
        0 => EntryType::Event,
        1 => EntryType::Level,
        2 => EntryType::End,
        3 => EntryType::Credits,
        k => {
            return Err(Error::invalid(format!(
                "unknown story entry type {k} at {rva:#x}"
            )))
        }
    };
    let entry = Entry {
        rva,
        kind,
        character: bytes[1] as i8,
        level: word(2),
        event_words: std::array::from_fn(|i| word(4 + 2 * i)),
    };
    // End stops the retail array reader. Its remaining ten bytes are data, not event IDs.
    if kind == EntryType::End {
        return Ok(entry);
    }
    if !matches!(entry.character, 0..=7 | 9 | 10) || !matches!(entry.level, 0..=67 | 70 | 71 | 90) {
        return Err(Error::invalid(format!(
            "unsupported story character or level at {rva:#x}"
        )));
    }
    let count = entry.event_words.iter().take_while(|v| **v >= 0).count();
    if entry.event_words.get(count).is_some_and(|v| *v != -1)
        || (kind == EntryType::Event) != (count > 0)
    {
        return Err(Error::invalid(format!(
            "unsupported story event slots at {rva:#x}"
        )));
    }
    Ok(entry)
}

fn sequence(
    exe: &[u8],
    image: &PeImage,
    start: u32,
    source_filename: String,
    budget: &mut usize,
) -> Result<Sequence> {
    let mut entries = Vec::new();
    loop {
        if *budget == 0 || entries.len() >= 4096 {
            return Err(Error::invalid(
                "story sequence record limit or missing terminator",
            ));
        }
        *budget -= 1;
        let rva = start
            .checked_add(entries.len() as u32 * 12)
            .ok_or_else(|| Error::invalid("story record address overflow"))?;
        let pos = image.rva_to_offset(rva, 12)?;
        let bytes = exe
            .get(pos..pos + 12)
            .ok_or_else(|| Error::invalid("truncated story record"))?;
        let entry = decode(bytes.try_into().unwrap(), rva)?;
        if entry.kind == EntryType::End {
            return Ok(Sequence {
                start_rva: start,
                source_filename,
                entries,
                terminator: entry,
            });
        }
        entries.push(entry);
    }
}

pub fn read(exe: &[u8], config: &str) -> Result<BTreeMap<String, Sequence>> {
    let image = verify_image(exe)?;
    let mut result = BTreeMap::new();
    let mut budget = 16384;
    for (section, mut fields) in sections(config, &["type", "address", "filename"])? {
        if !fields
            .get("type")
            .is_some_and(|v| v.eq_ignore_ascii_case("storysequence"))
        {
            continue;
        }
        let address = u32::from_str_radix(
            fields
                .get("address")
                .ok_or_else(|| Error::invalid("missing story address"))?,
            16,
        )
        .map_err(|_| Error::invalid("invalid story address"))?;
        let file = fields
            .remove("filename")
            .ok_or_else(|| Error::invalid("missing story filename"))?;
        if result.len() >= 128 {
            return Err(Error::invalid("story sequence limit"));
        }
        result.insert(section, sequence(exe, &image, address, file, &mut budget)?);
    }
    if result.is_empty() {
        return Err(Error::invalid("no storysequence mappings"));
    }
    Ok(result)
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Token {
    cursor: u64,
    entry_index: usize,
    event_slot: usize,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Action {
    Event { event: i16 },
    Level { character: i8, level: i16 },
    Credits,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dispatch {
    pub token: Token,
    pub source_rva: u32,
    pub action: Action,
}

static NEXT_CURSOR: AtomicU64 = AtomicU64::new(1);

pub struct Cursor<'a> {
    sequence: &'a Sequence,
    id: u64,
    index: usize,
    event_slot: usize,
    pending: Option<Dispatch>,
}
impl<'a> Cursor<'a> {
    pub fn new(sequence: &'a Sequence) -> Self {
        Self {
            sequence,
            id: NEXT_CURSOR.fetch_add(1, Ordering::Relaxed),
            index: 0,
            event_slot: 0,
            pending: None,
        }
    }
    pub fn finished(&self) -> bool {
        self.index == self.sequence.entries.len()
    }
    pub fn dispatch(&mut self) -> Option<Dispatch> {
        if let Some(pending) = &self.pending {
            return Some(pending.clone());
        }
        let entry = self.sequence.entries.get(self.index)?;
        let action = match entry.kind {
            EntryType::Event => Action::Event {
                event: entry.event_words[self.event_slot],
            },
            EntryType::Level => Action::Level {
                character: entry.character,
                level: entry.level,
            },
            EntryType::Credits => Action::Credits,
            EntryType::End => return None,
        };
        let dispatch = Dispatch {
            token: Token {
                cursor: self.id,
                entry_index: self.index,
                event_slot: self.event_slot,
            },
            source_rva: entry.rva,
            action,
        };
        self.pending = Some(dispatch.clone());
        Some(dispatch)
    }
    /// Confirms that the offline consumer handled this one dispatch.
    /// A level action itself makes no claim about the result of playing that level.
    pub fn acknowledge(&mut self, token: &Token) -> Result<()> {
        let Some(dispatch) = &self.pending else {
            return Err(Error::invalid("no outstanding story dispatch"));
        };
        if &dispatch.token != token {
            return Err(Error::invalid("story acknowledgment token mismatch"));
        }
        let entry = &self.sequence.entries[self.index];
        if entry.kind == EntryType::Event
            && entry
                .event_words
                .get(self.event_slot + 1)
                .is_some_and(|x| *x >= 0)
        {
            self.event_slot += 1;
        } else {
            self.index += 1;
            self.event_slot = 0;
        }
        self.pending = None;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn record(kind: i8, character: i8, level: i16, events: [i16; 4]) -> [u8; 12] {
        let mut data = [0; 12];
        data[0] = kind as u8;
        data[1] = character as u8;
        data[2..4].copy_from_slice(&level.to_le_bytes());
        for (chunk, word) in data[4..].as_chunks_mut::<2>().0.iter_mut().zip(events) {
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        data
    }
    #[test]
    fn rejects_unknown_types_ids_and_event_layouts() {
        for row in [
            record(4, 0, 0, [-1; 4]),
            record(1, 8, 13, [-1; 4]),
            record(1, 0, 91, [-1; 4]),
            record(0, 0, 0, [-1, 2, -1, -1]),
            record(0, 0, 0, [1, -2, -1, -1]),
            record(0, 0, 0, [-1; 4]),
            record(3, 0, 0, [1, -1, -1, -1]),
        ] {
            assert!(decode(&row, 12).is_err());
        }
        let e = decode(&record(2, -2, -3, [0, -1, -2, 3]), 24).unwrap();
        assert_eq!(
            (e.character, e.level, e.event_words),
            (-2, -3, [0, -1, -2, 3])
        );
    }
    #[test]
    fn cursor_requires_exact_outstanding_token_and_preserves_postcredits_event() {
        let rows = [
            record(0, 0, 0, [0, 12, -1, -1]),
            record(1, 4, 16, [-1; 4]),
            record(3, 0, 0, [-1; 4]),
            record(0, 0, 0, [429, -1, -1, -1]),
        ];
        let seq = Sequence {
            start_rva: 0,
            source_filename: "example".into(),
            entries: rows
                .iter()
                .enumerate()
                .map(|(i, row)| decode(row, i as u32 * 12).unwrap())
                .collect(),
            terminator: decode(&record(2, 0, 0, [0; 4]), 48).unwrap(),
        };
        let mut c = Cursor::new(&seq);
        let mut other = Cursor::new(&seq);
        let foreign = other.dispatch().unwrap();
        let mut old = None;
        for action in [
            Action::Event { event: 0 },
            Action::Event { event: 12 },
            Action::Level {
                character: 4,
                level: 16,
            },
            Action::Credits,
            Action::Event { event: 429 },
        ] {
            let d = c.dispatch().unwrap();
            assert_eq!(d.action, action);
            if let Some(old) = old.as_ref() {
                assert!(c.acknowledge(old).is_err());
            }
            assert!(c.acknowledge(&foreign.token).is_err());
            assert_eq!(c.dispatch(), Some(d.clone()));
            c.acknowledge(&d.token).unwrap();
            old = Some(d.token);
        }
        assert!(c.finished());
        assert!(c.dispatch().is_none());
        assert!(c.acknowledge(&old.unwrap()).is_err());
    }
}
