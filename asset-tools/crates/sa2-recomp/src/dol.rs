//! DOL executable and decomp-toolkit `symbols.txt` readers.

#[derive(Debug, Clone)]
pub struct Section {
    pub addr: u32,
    pub data: Vec<u8>,
    pub text: bool,
}

#[derive(Debug, Clone)]
pub struct Dol {
    pub sections: Vec<Section>,
    pub bss_addr: u32,
    pub bss_size: u32,
    pub entry: u32,
}

fn be(b: &[u8], o: usize) -> u32 {
    u32::from_be_bytes(b[o..o + 4].try_into().unwrap())
}

impl Dol {
    pub fn parse(b: &[u8]) -> Result<Dol, String> {
        if b.len() < 0x100 {
            return Err("DOL shorter than its header".into());
        }
        let mut sections = Vec::new();
        for i in 0..18 {
            let off = be(b, i * 4) as usize;
            let addr = be(b, 0x48 + i * 4);
            let size = be(b, 0x90 + i * 4) as usize;
            if size == 0 {
                continue;
            }
            let data = b
                .get(off..off + size)
                .ok_or_else(|| format!("section {i} runs past the file"))?
                .to_vec();
            sections.push(Section { addr, data, text: i < 7 });
        }
        Ok(Dol { sections, bss_addr: be(b, 0xD8), bss_size: be(b, 0xDC), entry: be(b, 0xE0) })
    }

    pub fn word(&self, a: u32) -> Option<u32> {
        self.sections.iter().find_map(|s| {
            let o = a.checked_sub(s.addr)? as usize;
            (o + 4 <= s.data.len()).then(|| be(&s.data, o))
        })
    }

    /// Copy the sections into memory and clear .bss, as the IPL does.
    pub fn load(&self, m: &mut crate::rt::Mem) {
        for i in 0..self.bss_size {
            m.w8(self.bss_addr + i, 0);
        }
        for s in &self.sections {
            m.load(s.addr, &s.data);
        }
    }
}

#[derive(Debug, Clone)]
pub struct RelSection {
    /// File offset (0 for .bss).
    pub offset: u32,
    pub size: u32,
    pub exec: bool,
}

/// One relocation: patch `section`+`offset` with `module`'s `target_section`
/// + `addend` (module 0 = main.dol, where `addend` is the absolute address).
#[derive(Debug, Clone, PartialEq)]
pub struct RelReloc {
    pub section: u8,
    pub offset: u32,
    pub kind: u8,
    pub module: u32,
    pub target_section: u8,
    pub addend: u32,
}

/// A GameCube relocatable module (.rel), as stored on disc (unlinked).
#[derive(Debug, Clone)]
pub struct Rel {
    pub id: u32,
    pub sections: Vec<RelSection>,
    pub relocs: Vec<RelReloc>,
    pub bytes: Vec<u8>,
}

fn be_at(b: &[u8], o: usize) -> Result<u32, String> {
    b.get(o..o + 4)
        .map(|s| u32::from_be_bytes(s.try_into().unwrap()))
        .ok_or_else(|| format!("REL read past the end at {o:#x}"))
}

impl Rel {
    pub fn parse(b: &[u8]) -> Result<Rel, String> {
        let id = be_at(b, 0)?;
        let num = be_at(b, 0xC)? as usize;
        let info = be_at(b, 0x10)? as usize;
        if num > 64 {
            return Err(format!("REL claims {num} sections"));
        }
        let mut sections = Vec::new();
        for i in 0..num {
            let off = be_at(b, info + 8 * i)?;
            let size = be_at(b, info + 8 * i + 4)?;
            let offset = off & !1;
            if offset != 0 && (offset as usize).checked_add(size as usize).map_or(true, |e| e > b.len()) {
                return Err(format!("REL section {i} runs past the file"));
            }
            sections.push(RelSection { offset, size, exec: off & 1 != 0 });
        }
        let (imp, imp_size) = (be_at(b, 0x28)? as usize, be_at(b, 0x2C)? as usize);
        let mut relocs = Vec::new();
        for k in 0..imp_size / 8 {
            let module = be_at(b, imp + 8 * k)?;
            let mut o = be_at(b, imp + 8 * k + 4)? as usize;
            let (mut section, mut pos) = (0u8, 0u32);
            loop {
                let e = b.get(o..o + 8).ok_or("REL relocation list runs past the file")?;
                o += 8;
                let step = u16::from_be_bytes([e[0], e[1]]) as u32;
                let (kind, sec) = (e[2], e[3]);
                let addend = u32::from_be_bytes([e[4], e[5], e[6], e[7]]);
                match kind {
                    203 => break, // R_DOLPHIN_END
                    202 => {
                        section = sec; // R_DOLPHIN_SECTION
                        pos = 0;
                    }
                    201 => pos += step, // R_DOLPHIN_NOP
                    _ => {
                        pos += step;
                        relocs.push(RelReloc { section, offset: pos, kind, module, target_section: sec, addend });
                    }
                }
            }
        }
        Ok(Rel { id, sections, relocs, bytes: b.to_vec() })
    }

    /// Big-endian word at `off` in section `s`.
    pub fn word(&self, s: usize, off: u32) -> Option<u32> {
        let sec = self.sections.get(s)?;
        if sec.offset == 0 || off.checked_add(4)? > sec.size {
            return None;
        }
        be_at(&self.bytes, (sec.offset + off) as usize).ok()
    }
}

/// SHA-1, to match a REL to the decomp's per-module hash (symbols from another
/// build would translate garbage).
pub fn sha1(data: &[u8]) -> String {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(chunk[4 * i..4 * i + 4].try_into().unwrap());
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let t = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e]) {
            *x = x.wrapping_add(y);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub section: String,
    pub addr: u32,
    pub size: u32,
    pub function: bool,
}

/// Parse decomp-toolkit symbols.txt lines:
/// `name = .text:0x80003100; // type:function size:0x10 scope:global`
pub fn parse_symbols(text: &str) -> Vec<Symbol> {
    text.lines()
        .filter_map(|l| {
            let (name, rest) = l.split_once(" = ")?;
            let (loc, attrs) = rest.split_once(';')?;
            let (section, addr) = loc.split_once(':')?;
            let addr = u32::from_str_radix(addr.trim().trim_start_matches("0x"), 16).ok()?;
            let mut size = 0;
            let mut function = false;
            for a in attrs.trim_start_matches(|c| c == ' ' || c == '/').split_whitespace() {
                if let Some(v) = a.strip_prefix("size:0x") {
                    size = u32::from_str_radix(v, 16).unwrap_or(0);
                }
                if a == "type:function" {
                    function = true;
                }
            }
            Some(Symbol { name: name.trim().into(), section: section.trim().into(), addr, size, function })
        })
        .collect()
}
