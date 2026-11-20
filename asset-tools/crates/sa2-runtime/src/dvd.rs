//! The disc as the game sees it (DVDOpen/DVDRead), served from the player's
//! Steam files. A GameCube path (`/stg11D.rel`, `sound/foo.mlt`) resolves to
//! `resource/gd_PC/<path>` ignoring case, or, failing that, to the only file
//! under `gd_PC` with that name. Files come through [`GameFiles`], so the
//! browser can hand in what its folder picker read and the CLI a folder.
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Where the game's files come from. Paths are relative to the game folder,
/// with `/` separators (`resource/gd_PC/stg11D.rel`).
pub trait GameFiles {
    /// Every file path available.
    fn paths(&self) -> Vec<String>;
    fn size(&self, path: &str) -> Option<u64>;
    /// Bytes at `offset` into `buf`; how many were read (short at the end).
    fn read_at(&self, path: &str, offset: u64, buf: &mut [u8]) -> Option<usize>;
}

impl<T: GameFiles + ?Sized> GameFiles for Box<T> {
    fn paths(&self) -> Vec<String> {
        (**self).paths()
    }
    fn size(&self, path: &str) -> Option<u64> {
        (**self).size(path)
    }
    fn read_at(&self, path: &str, offset: u64, buf: &mut [u8]) -> Option<usize> {
        (**self).read_at(path, offset, buf)
    }
}

/// Files already in memory (the browser's folder picker).
#[derive(Default)]
pub struct MemFiles(pub BTreeMap<String, Vec<u8>>);

impl GameFiles for MemFiles {
    fn paths(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }
    fn size(&self, path: &str) -> Option<u64> {
        self.0.get(path).map(|b| b.len() as u64)
    }
    fn read_at(&self, path: &str, offset: u64, buf: &mut [u8]) -> Option<usize> {
        let b = self.0.get(path)?;
        let rest = b.get(usize::try_from(offset).ok()?..)?;
        let n = rest.len().min(buf.len());
        buf[..n].copy_from_slice(&rest[..n]);
        Some(n)
    }
}

/// The game folder on disk; only `resource/gd_PC` is listed.
pub struct DirFiles {
    pub root: PathBuf,
}

impl GameFiles for DirFiles {
    fn paths(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec!["resource/gd_PC".to_string()];
        while let Some(dir) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(self.root.join(&dir)) else {
                continue;
            };
            for e in rd.flatten() {
                let name = format!("{dir}/{}", e.file_name().to_string_lossy());
                match e.file_type() {
                    Ok(t) if t.is_dir() => stack.push(name),
                    Ok(t) if t.is_file() => out.push(name),
                    _ => {}
                }
            }
        }
        out
    }
    fn size(&self, path: &str) -> Option<u64> {
        std::fs::metadata(self.root.join(path))
            .ok()
            .map(|m| m.len())
    }
    fn read_at(&self, path: &str, offset: u64, buf: &mut [u8]) -> Option<usize> {
        use std::io::{Read, Seek, SeekFrom};
        let mut f = std::fs::File::open(self.root.join(path)).ok()?;
        f.seek(SeekFrom::Start(offset)).ok()?;
        let mut n = 0;
        while n < buf.len() {
            match f.read(&mut buf[n..]) {
                Ok(0) => break,
                Ok(k) => n += k,
                Err(_) => return None,
            }
        }
        Some(n)
    }
}

const GD: &str = "resource/gd_pc/";

/// Open files by entry number, as `DVDConvertPathToEntrynum` hands them out.
pub struct Dvd<F> {
    pub files: F,
    /// Lower-case path under gd_PC, and bare file name when unique -> real path.
    index: BTreeMap<String, String>,
    entries: Vec<String>,
}

impl<F: GameFiles> Dvd<F> {
    pub fn new(files: F) -> Self {
        let mut index = BTreeMap::new();
        let mut names: BTreeMap<String, Option<String>> = BTreeMap::new();
        for p in files.paths() {
            let lower = p.replace('\\', "/").to_lowercase();
            let Some(rel) = lower.strip_prefix(GD) else {
                continue;
            };
            if let Some(name) = rel.rsplit('/').next() {
                names
                    .entry(name.to_string())
                    .and_modify(|v| *v = None)
                    .or_insert_with(|| Some(p.clone()));
            }
            index.insert(rel.to_string(), p);
        }
        for (name, p) in names {
            if let Some(p) = p {
                index.entry(name).or_insert(p);
            }
        }
        Self {
            files,
            index,
            entries: Vec::new(),
        }
    }

    /// `DVDConvertPathToEntrynum`: the entry for a disc path, if a Steam
    /// file answers to it.
    pub fn entry(&mut self, gc_path: &str) -> Option<i32> {
        let key = gc_path
            .replace('\\', "/")
            .trim_start_matches('/')
            .to_lowercase();
        let real = self.index.get(&key)?;
        let i = match self.entries.iter().position(|e| e == real) {
            Some(i) => i,
            None => {
                self.entries.push(real.clone());
                self.entries.len() - 1
            }
        };
        i32::try_from(i).ok()
    }

    /// The Steam path behind an entry.
    pub fn path(&self, entry: i32) -> Option<&str> {
        self.entries
            .get(usize::try_from(entry).ok()?)
            .map(String::as_str)
    }

    pub fn length(&self, entry: i32) -> Option<u64> {
        self.files.size(self.path(entry)?)
    }

    /// `DVDRead`: bytes at `offset` of the entry's file.
    pub fn read(&self, entry: i32, offset: u64, buf: &mut [u8]) -> Option<usize> {
        self.files.read_at(self.path(entry)?, offset, buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dvd() -> Dvd<MemFiles> {
        let mut f = MemFiles::default();
        f.0.insert("resource/gd_PC/stg11D.rel".into(), vec![1, 2, 3, 4]);
        f.0.insert("resource/gd_PC/event/MLT/e0000/a.mlt".into(), vec![9]);
        f.0.insert("resource/gd_PC/x/dup.bin".into(), vec![]);
        f.0.insert("resource/gd_PC/y/dup.bin".into(), vec![]);
        f.0.insert("sonic2app.exe".into(), vec![]);
        Dvd::new(f)
    }

    #[test]
    fn paths_resolve_ignoring_case_and_slash() {
        let mut d = dvd();
        let e = d.entry("/STG11D.REL").unwrap();
        assert_eq!(d.entry("stg11d.rel"), Some(e));
        assert_eq!(d.length(e), Some(4));
        let mut b = [0u8; 8];
        assert_eq!(d.read(e, 1, &mut b), Some(3));
        assert_eq!(&b[..3], &[2, 3, 4]);
        assert_eq!(d.read(e, 9, &mut b), None);
    }

    #[test]
    fn bare_names_only_when_unique() {
        let mut d = dvd();
        assert!(d.entry("/event/mlt/e0000/a.mlt").is_some());
        assert_eq!(d.entry("a.mlt"), d.entry("event/MLT/e0000/a.mlt"));
        assert_eq!(d.entry("dup.bin"), None);
        assert!(d.entry("x/dup.bin").is_some());
        assert_eq!(d.entry("sonic2app.exe"), None);
        assert_eq!(d.entry("missing"), None);
    }
}
