//! Read-only CRI `@UTF` tables and the Steam CSB sound banks built from them.
//!
//! On Steam each GameCube `.mlt`/`.mpb` bank is replaced by a folder of
//! numbered `.csb` files with the same bank name (`MLT/<bank>/<n>.csb`,
//! `MPB/<bank>/<n>.csb`, `event/MLT/<event>/<n>.csb`). A CSB root table lists
//! named sub-tables; `CUE` names each sound, `SYNTH` links it to a waveform and
//! `SOUND_ELEMENT` holds the waveform as `AAX` segments of plain CRI ADX,
//! which `audio::decode_adx` decodes. Layout established from the installed
//! files themselves (2026-11-20 survey: 283 banks, 6,577 elements).
use crate::{Error, Result};
use serde::Serialize;

const MAX_COLUMNS: usize = 1024;
const MAX_ROWS: usize = 1 << 16;
const MAX_DEPTH: usize = 4;

fn span(input: &[u8], at: usize, len: usize) -> Result<&[u8]> {
    input
        .get(
            at..at
                .checked_add(len)
                .ok_or_else(|| Error::invalid("UTF range overflow"))?,
        )
        .ok_or_else(|| Error::invalid("truncated UTF table"))
}
fn be16(input: &[u8], at: usize) -> Result<u16> {
    let b = span(input, at, 2)?;
    Ok(u16::from_be_bytes([b[0], b[1]]))
}
fn be32(input: &[u8], at: usize) -> Result<u32> {
    let b = span(input, at, 4)?;
    Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}
fn be64(input: &[u8], at: usize) -> Result<u64> {
    let b = span(input, at, 8)?;
    let mut w = [0; 8];
    w.copy_from_slice(b);
    Ok(u64::from_be_bytes(w))
}

/// One cell. `Data` offsets are absolute in the slice handed to [`read_utf`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Value {
    Zero,
    Unsigned(u64),
    Signed(i64),
    Float(f32),
    Text(String),
    Data { offset: usize, length: usize },
}

#[derive(Debug, Clone, Serialize)]
pub struct Table {
    pub name: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

impl Table {
    pub fn column(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c == name)
    }
    pub fn get(&self, row: usize, name: &str) -> Option<&Value> {
        self.rows.get(row)?.get(self.column(name)?)
    }
    fn text(&self, row: usize, name: &str) -> Result<String> {
        match self.get(row, name) {
            Some(Value::Text(s)) => Ok(s.clone()),
            Some(Value::Zero) => Ok(String::new()),
            _ => Err(Error::invalid(format!(
                "{} row {row}: {name} is not text",
                self.name
            ))),
        }
    }
    fn unsigned(&self, row: usize, name: &str) -> Result<u64> {
        match self.get(row, name) {
            Some(Value::Unsigned(v)) => Ok(*v),
            Some(Value::Signed(v)) if *v >= 0 => Ok(*v as u64),
            Some(Value::Zero) => Ok(0),
            _ => Err(Error::invalid(format!(
                "{} row {row}: {name} is not a count",
                self.name
            ))),
        }
    }
    fn data(&self, row: usize, name: &str) -> Result<(usize, usize)> {
        match self.get(row, name) {
            Some(Value::Data { offset, length }) => Ok((*offset, *length)),
            Some(Value::Zero) => Ok((0, 0)),
            _ => Err(Error::invalid(format!(
                "{} row {row}: {name} is not data",
                self.name
            ))),
        }
    }
}

fn width(kind: u8) -> Result<usize> {
    Ok(match kind {
        0 | 1 => 1,
        2 | 3 => 2,
        4 | 5 | 8 | 10 => 4,
        6 | 7 | 11 => 8,
        _ => return Err(Error::invalid("unsupported UTF column type")),
    })
}

/// Read the `@UTF` table that starts at `base`. Strings are Shift-JIS.
pub fn read_utf(input: &[u8], base: usize) -> Result<Table> {
    if span(input, base, 4)? != b"@UTF" {
        return Err(Error::invalid("expected @UTF table"));
    }
    let size = be32(input, base + 4)? as usize;
    let t = base + 8;
    let table = span(input, t, size)?;
    let rows_at = be16(table, 2)? as usize;
    let strings_at = be32(table, 4)? as usize;
    let data_at = be32(table, 8)? as usize;
    let name_at = be32(table, 12)? as usize;
    let columns = be16(table, 16)? as usize;
    let row_width = be16(table, 18)? as usize;
    let rows = be32(table, 20)? as usize;
    if columns > MAX_COLUMNS || rows > MAX_ROWS || strings_at > size || data_at > size {
        return Err(Error::invalid("UTF table header out of range"));
    }
    let strings = &table[strings_at..data_at.max(strings_at)];
    let text = |at: usize| -> Result<String> {
        let rest = strings
            .get(at..)
            .ok_or_else(|| Error::invalid("UTF string outside string pool"))?;
        let end = rest
            .iter()
            .position(|b| *b == 0)
            .ok_or_else(|| Error::invalid("unterminated UTF string"))?;
        let (s, _, bad) = encoding_rs::SHIFT_JIS.decode(&rest[..end]);
        if bad {
            return Err(Error::invalid("malformed UTF string"));
        }
        Ok(s.into_owned())
    };
    let value = |at: usize, kind: u8| -> Result<Value> {
        Ok(match kind {
            0 => Value::Unsigned(span(table, at, 1)?[0] as u64),
            1 => Value::Signed(span(table, at, 1)?[0] as i8 as i64),
            2 => Value::Unsigned(be16(table, at)? as u64),
            3 => Value::Signed(be16(table, at)? as i16 as i64),
            4 => Value::Unsigned(be32(table, at)? as u64),
            5 => Value::Signed(be32(table, at)? as i32 as i64),
            6 => Value::Unsigned(be64(table, at)?),
            7 => Value::Signed(be64(table, at)? as i64),
            8 => Value::Float(f32::from_bits(be32(table, at)?)),
            10 => Value::Text(text(be32(table, at)? as usize)?),
            11 => {
                let offset = be32(table, at)? as usize;
                let length = be32(table, at + 4)? as usize;
                let start = data_at
                    .checked_add(offset)
                    .ok_or_else(|| Error::invalid("UTF data overflow"))?;
                span(table, start, length)?;
                Value::Data {
                    offset: t + start,
                    length,
                }
            }
            _ => return Err(Error::invalid("unsupported UTF column type")),
        })
    };
    // Column descriptors: flags (storage << 4 | type), name offset, optional constant.
    let mut at = 24;
    let mut specs = Vec::with_capacity(columns);
    for _ in 0..columns {
        let flags = span(table, at, 1)?[0];
        let name = text(be32(table, at + 1)? as usize)?;
        at += 5;
        let (storage, kind) = (flags >> 4, flags & 15);
        let constant = match storage {
            1 => Some(Value::Zero),
            3 => {
                let v = value(at, kind)?;
                at += width(kind)?;
                Some(v)
            }
            5 => {
                width(kind)?;
                None
            }
            _ => return Err(Error::invalid("unsupported UTF column storage")),
        };
        specs.push((name, kind, constant));
    }
    let per_row: usize = specs
        .iter()
        .filter(|s| s.2.is_none())
        .map(|s| width(s.1))
        .sum::<Result<usize>>()?;
    if per_row > row_width {
        return Err(Error::invalid("UTF row wider than declared"));
    }
    let mut out = Vec::with_capacity(rows);
    for r in 0..rows {
        let mut p = rows_at
            .checked_add(
                r.checked_mul(row_width)
                    .ok_or_else(|| Error::invalid("UTF row overflow"))?,
            )
            .ok_or_else(|| Error::invalid("UTF row overflow"))?;
        let mut row = Vec::with_capacity(specs.len());
        for (_, kind, constant) in &specs {
            match constant {
                Some(v) => row.push(v.clone()),
                None => {
                    row.push(value(p, *kind)?);
                    p += width(*kind)?;
                }
            }
        }
        out.push(row);
    }
    Ok(Table {
        name: text(name_at)?,
        columns: specs.into_iter().map(|s| s.0).collect(),
        rows: out,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cue {
    pub name: String,
    pub id: u64,
    pub synth: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Synth {
    pub name: String,
    /// Waveform (`SOUND_ELEMENT` name) or another synth this one plays.
    pub link: String,
}

/// One stored waveform segment, as an absolute byte range in the CSB file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub offset: usize,
    pub length: usize,
    /// Value of the AAX `lpflg` column (the looped part when non-zero).
    pub loop_flag: u64,
    /// True when the payload carries the ADX signature (0x8000).
    pub adx: bool,
    /// True when the first ADX frame is already the end marker (0x8001): a
    /// stub with no audio, although its header may declare a few samples.
    pub silent: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Element {
    pub name: String,
    pub format: u64,
    pub channels: u64,
    pub streamed: u64,
    pub sample_rate: u64,
    pub sample_count: u64,
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bank {
    pub tables: Vec<String>,
    pub cues: Vec<Cue>,
    pub synths: Vec<Synth>,
    pub elements: Vec<Element>,
}

fn sub_table(input: &[u8], root: &Table, name: &str) -> Result<Option<Table>> {
    for r in 0..root.rows.len() {
        if root.text(r, "name")? == name {
            let (offset, length) = root.data(r, "utf")?;
            if length == 0 {
                return Ok(None);
            }
            return read_utf(span(input, 0, offset + length)?, offset).map(Some);
        }
    }
    Ok(None)
}

/// Read a CSB bank's cues, synth links and waveform segments.
pub fn read(input: &[u8]) -> Result<Bank> {
    let root = read_utf(input, 0)?;
    if root.name != "TBLCSB" {
        return Err(Error::invalid("expected a TBLCSB root table"));
    }
    let tables = (0..root.rows.len())
        .map(|r| root.text(r, "name"))
        .collect::<Result<Vec<_>>>()?;
    let mut cues = Vec::new();
    if let Some(t) = sub_table(input, &root, "CUE")? {
        for r in 0..t.rows.len() {
            cues.push(Cue {
                name: t.text(r, "name")?,
                id: t.unsigned(r, "id")?,
                synth: t.text(r, "synth")?,
            });
        }
    }
    let mut synths = Vec::new();
    if let Some(t) = sub_table(input, &root, "SYNTH")? {
        for r in 0..t.rows.len() {
            synths.push(Synth {
                name: t.text(r, "synname")?,
                link: t.text(r, "lnkname")?,
            });
        }
    }
    let mut elements = Vec::new();
    if let Some(t) = sub_table(input, &root, "SOUND_ELEMENT")? {
        for r in 0..t.rows.len() {
            let (offset, length) = t.data(r, "data")?;
            let segments = if length == 0 {
                Vec::new()
            } else {
                aax_segments(input, offset, length, 0)?
            };
            elements.push(Element {
                name: t.text(r, "name")?,
                format: t.unsigned(r, "fmt")?,
                channels: t.unsigned(r, "nch")?,
                streamed: t.unsigned(r, "stmflg")?,
                sample_rate: t.unsigned(r, "sfreq")?,
                sample_count: t.unsigned(r, "nsmpl")?,
                segments,
            });
        }
    }
    Ok(Bank {
        tables,
        cues,
        synths,
        elements,
    })
}

fn aax_segments(input: &[u8], offset: usize, length: usize, depth: usize) -> Result<Vec<Segment>> {
    if depth > MAX_DEPTH {
        return Err(Error::invalid("nested AAX tables too deep"));
    }
    let end = offset
        .checked_add(length)
        .ok_or_else(|| Error::invalid("AAX range overflow"))?;
    let bytes = span(input, 0, end)?;
    let payload = &bytes[offset..];
    if !payload.starts_with(b"@UTF") {
        return Ok(segment(payload, offset, 0).into_iter().collect());
    }
    let aax = read_utf(bytes, offset)?;
    let mut out = Vec::new();
    for r in 0..aax.rows.len() {
        let (o, n) = aax.data(r, "data")?;
        let loop_flag = aax.unsigned(r, "lpflg").unwrap_or(0);
        let part = span(input, o, n)?;
        if part.starts_with(b"@UTF") {
            out.extend(aax_segments(input, o, n, depth + 1)?);
        } else {
            out.extend(segment(part, o, loop_flag));
        }
    }
    Ok(out)
}

/// A stored segment; an empty data cell (a bank's placeholder sound) has none.
fn segment(payload: &[u8], offset: usize, loop_flag: u64) -> Option<Segment> {
    if payload.is_empty() {
        return None;
    }
    let adx = payload.starts_with(&[0x80, 0]);
    // The ADX header length is the big-endian word at +2; audio starts 4 bytes later.
    // A stub's first frame is the end marker and its header declares no more
    // samples than one frame holds (all 2,729 Steam stubs declare 1-32).
    let silent = adx
        && payload
            .get(2..4)
            .map(|w| u16::from_be_bytes([w[0], w[1]]) as usize + 4)
            .and_then(|start| payload.get(start..start + 2))
            == Some(&[0x80, 1][..])
        && match (payload.get(5), payload.get(12..16)) {
            (Some(&block), Some(t)) => {
                let total = u32::from_be_bytes([t[0], t[1], t[2], t[3]]) as u64;
                total <= (block as u64).saturating_sub(2) * 2
            }
            _ => false,
        };
    Some(Segment {
        offset,
        length: payload.len(),
        loop_flag,
        adx,
        silent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic one-table `@UTF` image: columns are (name, flags, per-row bytes).
    fn table(
        name: &str,
        cols: &[(&str, u8)],
        rows: &[Vec<u8>],
        strings: &[&str],
        data: &[u8],
    ) -> Vec<u8> {
        let mut pool = b"<NULL>\0".to_vec();
        fn off(s: &str, pool: &mut Vec<u8>) -> u32 {
            let at = pool.len() as u32;
            pool.extend(s.as_bytes());
            pool.push(0);
            at
        }
        let name_at = off(name, &mut pool);
        let col_names: Vec<u32> = cols.iter().map(|c| off(c.0, &mut pool)).collect();
        for s in strings {
            off(s, &mut pool);
        }
        let width = rows.first().map_or(0, |r| r.len());
        let mut head = Vec::new();
        for (c, at) in cols.iter().zip(&col_names) {
            head.push(c.1);
            head.extend(at.to_be_bytes());
        }
        let rows_at = 24 + head.len();
        let strings_at = rows_at + width * rows.len();
        let data_at = strings_at + pool.len();
        let mut t = vec![0, 0];
        t.extend((rows_at as u16).to_be_bytes());
        t.extend((strings_at as u32).to_be_bytes());
        t.extend((data_at as u32).to_be_bytes());
        t.extend(name_at.to_be_bytes());
        t.extend((cols.len() as u16).to_be_bytes());
        t.extend((width as u16).to_be_bytes());
        t.extend((rows.len() as u32).to_be_bytes());
        t.extend(head);
        for r in rows {
            t.extend(r);
        }
        t.extend(pool);
        t.extend(data);
        let mut out = b"@UTF".to_vec();
        out.extend((t.len() as u32).to_be_bytes());
        out.extend(t);
        out
    }

    #[test]
    fn reads_columns_constants_and_strings() {
        // u16 per-row, zero column, string per-row (pool offset of "hello").
        let pool_hello = 7 + "T\0".len() + "a\0".len() + "z\0".len() + "s\0".len();
        let rows = vec![
            [
                1u16.to_be_bytes().to_vec(),
                (pool_hello as u32).to_be_bytes().to_vec(),
            ]
            .concat(),
            [2u16.to_be_bytes().to_vec(), 0u32.to_be_bytes().to_vec()].concat(),
        ];
        let b = table(
            "T",
            &[("a", 0x52), ("z", 0x10), ("s", 0x5a)],
            &rows,
            &["hello"],
            &[],
        );
        let t = read_utf(&b, 0).unwrap();
        assert_eq!(t.name, "T");
        assert_eq!(t.columns, ["a", "z", "s"]);
        assert_eq!(t.rows[1][0], Value::Unsigned(2));
        assert_eq!(t.rows[0][1], Value::Zero);
        assert_eq!(t.rows[0][2], Value::Text("hello".into()));
        assert_eq!(t.rows[1][2], Value::Text("<NULL>".into()));
    }

    #[test]
    fn data_cells_are_absolute_and_bounded() {
        let rows = vec![[0u32.to_be_bytes(), 3u32.to_be_bytes()].concat()];
        let mut b = vec![0xee; 5];
        b.extend(table("D", &[("data", 0x5b)], &rows, &[], b"xyz"));
        let t = read_utf(&b, 5).unwrap();
        let Value::Data { offset, length } = t.rows[0][0] else {
            panic!()
        };
        assert_eq!(&b[offset..offset + length], b"xyz");
        let bad = vec![[0u32.to_be_bytes(), 9u32.to_be_bytes()].concat()];
        assert!(read_utf(&table("D", &[("data", 0x5b)], &bad, &[], b"xyz"), 0).is_err());
    }

    #[test]
    fn truncation_and_unknown_types_never_panic() {
        let rows = vec![1u16.to_be_bytes().to_vec()];
        let b = table("T", &[("a", 0x52)], &rows, &[], &[]);
        for n in 0..b.len() {
            assert!(read_utf(&b[..n], 0).is_err());
        }
        let mut odd = b.clone();
        odd[8 + 24] = 0x5c; // type 12 is not defined
        assert!(read_utf(&odd, 0).is_err());
        assert!(read(&b).is_err()); // not a TBLCSB root
    }

    /// Synthetic ADX: 0x24 header offset, 18-byte blocks, `total` declared samples,
    /// then one frame starting with `first`.
    fn adx(total: u32, first: [u8; 2]) -> Vec<u8> {
        let mut a = vec![0x80, 0, 0, 0x24, 3, 18, 4, 1];
        a.extend(22050u32.to_be_bytes());
        a.extend(total.to_be_bytes());
        a.resize(0x28, 0);
        a.extend(first);
        a.resize(0x28 + 18, 0);
        a
    }

    #[test]
    fn stubs_are_silent_only_when_they_declare_one_frame_at_most() {
        assert!(segment(&adx(4, [0x80, 1]), 0, 0).unwrap().silent);
        assert!(segment(&adx(32, [0x80, 1]), 0, 0).unwrap().silent);
        // An end marker before a longer declared length is a damaged stream, not a stub.
        assert!(!segment(&adx(33, [0x80, 1]), 0, 0).unwrap().silent);
        assert!(!segment(&adx(4, [0x00, 7]), 0, 0).unwrap().silent);
        assert!(segment(&[], 0, 0).is_none());
        assert!(!segment(b"RIFF", 0, 0).unwrap().adx);
    }
}
