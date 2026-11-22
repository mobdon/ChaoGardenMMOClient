//! The disc as the game sees it (DVDOpen/DVDRead), served from the player's
//! Steam files. A GameCube path (`/stg11D.rel`, `sound/foo.mlt`) resolves to
//! `resource/gd_PC/<path>` ignoring case, or, failing that, to the only file
//! under `gd_PC` with that name. Files come through [`GameFiles`], so the
//! browser can hand in what its folder picker read and the CLI a folder.
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Where the game's files come from. Paths are relative to the game folder,
/// with `/` separators (`resource/gd_PC/stg11D.rel`).
pub trait GameFiles: Send {
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
        if let Some(f) = self.converted(path) {
            return Some(f.len() as u64);
        }
        self.raw_size(path)
    }
    fn read_at(&self, path: &str, offset: u64, buf: &mut [u8]) -> Option<usize> {
        if let Some(f) = self.converted(path) {
            let rest = f.get(usize::try_from(offset).ok()?..)?;
            let n = rest.len().min(buf.len());
            buf[..n].copy_from_slice(&rest[..n]);
            return Some(n);
        }
        self.raw_read_at(path, offset, buf)
    }
}

/// Steam's subtitle fonts, `efmsgfont_*.bin` (the disc names, any case).
fn is_font(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    name.starts_with("efmsgfont_") && name.ends_with(".bin")
}

const STEAM_GLYPH: usize = 24 * 24 * 4;
const GC_GLYPH: usize = 24 * 3;

/// Steam's subtitle font in the GameCube layout. Steam: 24x24 glyphs, 4 bytes
/// a pixel (equal colour bytes, coverage alpha in byte 3), in the disc's glyph
/// order. GameCube: 72-byte 1bpp glyphs, rows of 3 bytes read little-endian,
/// bit 0 = leftmost pixel. Trailing empty glyphs are dropped, which gives the
/// disc's counts for all three files (96, 224, 3,423 glyphs). The PC glyphs
/// are a different, antialiased drawing, so this is not byte-equal to the disc.
/// shortcut: alpha >= 0x80 counts as ink (no 1bpp Steam source exists); tune if subtitles look thin or bold.
pub fn steam_font_to_gc(steam: &[u8]) -> Option<Vec<u8>> {
    if steam.is_empty() || steam.len() % STEAM_GLYPH != 0 {
        return None;
    }
    let glyphs: Vec<&[u8]> = steam.chunks(STEAM_GLYPH).collect();
    let used = glyphs.iter().rposition(|g| g.iter().any(|b| *b != 0))? + 1;
    let mut out = Vec::with_capacity(used * GC_GLYPH);
    for g in &glyphs[..used] {
        for row in g.chunks(24 * 4) {
            let bits = row
                .chunks(4)
                .enumerate()
                .fold(0u32, |a, (x, px)| a | (u32::from(px[3] >= 0x80) << x));
            out.extend_from_slice(&bits.to_le_bytes()[..3]);
        }
    }
    Some(out)
}

type FontCache = std::sync::Mutex<BTreeMap<PathBuf, Option<std::sync::Arc<Vec<u8>>>>>;
static FONTS: std::sync::OnceLock<FontCache> = std::sync::OnceLock::new();

impl DirFiles {
    /// A Steam file in the GameCube layout, converted once and kept (the game
    /// reads it in pieces): subtitle fonts, and texture archives `<name>.prs`
    /// that have a Steam `PRS/<name>.pak`. `None`: serve the file as it is.
    fn converted(&self, path: &str) -> Option<std::sync::Arc<Vec<u8>>> {
        let pak = path
            .to_lowercase()
            .strip_suffix(".prs")
            .and_then(|p| p.rsplit_once('/'))
            // The HD objtex_common made Dry Lagoon (stage 18) crash at its first frame; Steam's own copy runs.
            .filter(|(_, stem)| *stem != "objtex_common")
            .map(|(dir, stem)| self.root.join(format!("{dir}/PRS/{stem}.pak")))
            .filter(|p| p.is_file());
        if !is_font(path) && pak.is_none() {
            return None;
        }
        let full = self.root.join(path);
        let mut cache = FONTS.get_or_init(Default::default).lock().ok()?;
        if let Some(f) = cache.get(&full) {
            return f.clone();
        }
        let raw = std::fs::read(&full).ok()?;
        let f = match pak {
            Some(pak) => std::fs::read(pak)
                .ok()
                .and_then(|p| crate::steam_tex::pak_to_prs(&p, &raw)),
            None => steam_font_to_gc(&raw),
        }
        .map(std::sync::Arc::new);
        cache.insert(full, f.clone());
        f
    }
    fn raw_size(&self, path: &str) -> Option<u64> {
        std::fs::metadata(self.root.join(path))
            .ok()
            .map(|m| m.len())
    }
    fn raw_read_at(&self, path: &str, offset: u64, buf: &mut [u8]) -> Option<usize> {
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

/// A flat folder of disc files (an extracted GameCube `files/`), served as
/// `resource/gd_PC/<name>` so it can stand behind the Steam folder.
pub struct FlatDir {
    pub dir: PathBuf,
}

impl GameFiles for FlatDir {
    fn paths(&self) -> Vec<String> {
        let Ok(rd) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        rd.flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| format!("resource/gd_PC/{}", e.file_name().to_string_lossy()))
            .collect()
    }
    fn size(&self, path: &str) -> Option<u64> {
        let name = path.rsplit('/').next()?;
        DirFiles {
            root: self.dir.clone(),
        }
        .raw_size(name)
    }
    fn read_at(&self, path: &str, offset: u64, buf: &mut [u8]) -> Option<usize> {
        let name = path.rsplit('/').next()?;
        DirFiles {
            root: self.dir.clone(),
        }
        .raw_read_at(name, offset, buf)
    }
}

/// Several sources in order; the first that has a path serves it (Steam
/// first, then the disc for files the Steam release dropped).
pub struct Layered(pub Vec<Box<dyn GameFiles>>);

impl GameFiles for Layered {
    fn paths(&self) -> Vec<String> {
        let mut seen = std::collections::BTreeSet::new();
        self.0
            .iter()
            .flat_map(|f| f.paths())
            .filter(|p| seen.insert(p.to_lowercase()))
            .collect()
    }
    fn size(&self, path: &str) -> Option<u64> {
        self.0.iter().find_map(|f| f.size(path))
    }
    fn read_at(&self, path: &str, offset: u64, buf: &mut [u8]) -> Option<usize> {
        self.0.iter().find_map(|f| f.read_at(path, offset, buf))
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

    #[test]
    fn steam_font_converts_to_gc_bits() {
        // Synthetic: glyph 0 inks pixel (row 1, x 0) and (row 1, x 23) above the
        // threshold, one pixel below it; glyph 1 is ink at (0, 9); glyph 2 empty.
        let mut s = vec![0u8; STEAM_GLYPH * 3];
        let px = |g: usize, y: usize, x: usize| g * STEAM_GLYPH + (y * 24 + x) * 4 + 3;
        s[px(0, 1, 0)] = 0x80;
        s[px(0, 1, 23)] = 0xff;
        s[px(0, 2, 5)] = 0x7f;
        s[px(1, 0, 9)] = 0xc0;
        let gc = steam_font_to_gc(&s).unwrap();
        assert_eq!(gc.len(), 2 * GC_GLYPH, "trailing empty glyph dropped");
        assert_eq!(&gc[3..6], &[0x01, 0x00, 0x80]);
        assert_eq!(&gc[6..9], &[0, 0, 0]);
        assert_eq!(&gc[GC_GLYPH..GC_GLYPH + 3], &[0x00, 0x02, 0x00]);
        assert_eq!(steam_font_to_gc(&s[..10]), None);
        assert_eq!(steam_font_to_gc(&vec![0; STEAM_GLYPH]), None);
    }

    #[test]
    fn only_efmsgfont_names_convert() {
        assert!(is_font("resource/gd_PC/EFMSGFONT_KANJI24.BIN"));
        assert!(is_font("resource/gd_PC/efmsgfont_ascii24E.bin"));
        assert!(!is_font("resource/gd_PC/efmsgfont_ascii24E.prs"));
        assert!(!is_font("resource/gd_PC/msgfont.bin"));
    }

    /// Local only: the installed Steam fonts convert to the disc's sizes.
    /// SA2_STEAM_DIR (default Steam path); SA2_DISC_DIR to compare sizes.
    #[test]
    #[ignore]
    fn installed_steam_fonts_match_disc_sizes() {
        let root = std::env::var("SA2_STEAM_DIR").unwrap_or_else(|_| {
            "C:/Program Files (x86)/Steam/steamapps/common/Sonic Adventure 2".into()
        });
        let steam = DirFiles { root: root.into() };
        for (name, glyphs) in [
            ("efmsgfont_ascii24E.bin", 96u64),
            ("efmsgfont_ascii24S.bin", 224),
            ("EFMSGFONT_KANJI24.BIN", 3423),
        ] {
            let path = format!("resource/gd_PC/{name}");
            assert_eq!(steam.size(&path), Some(glyphs * GC_GLYPH as u64), "{name}");
            if let Some(dir) = std::env::var_os("SA2_DISC_DIR") {
                let disc = FlatDir { dir: dir.into() };
                assert_eq!(disc.size(&path), steam.size(&path), "{name} vs disc");
            }
        }
    }
}
