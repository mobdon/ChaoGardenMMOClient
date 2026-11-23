// Read verified PC symbol mappings from SA2Variables.h; never modify the executable.
use std::{env, fs, io};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    if !(args.len() == 3 || args.len() == 4 && args[3] == "--sky-rails") {
        return Err("usage: sa2-roster sonic2app.exe SA2Variables.h".into());
    }
    let b = fs::read(&args[1])?;
    let header = fs::read_to_string(&args[2])?;
    let bad = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid PE32 image or unmapped table",
        )
    };
    let word = |i: usize| -> Result<u16, io::Error> {
        Ok(u16::from_le_bytes(
            b.get(i..i + 2).ok_or_else(bad)?.try_into().unwrap(),
        ))
    };
    let dword = |i: usize| -> Result<u32, io::Error> {
        Ok(u32::from_le_bytes(
            b.get(i..i + 4).ok_or_else(bad)?.try_into().unwrap(),
        ))
    };
    let pe = dword(60)? as usize;
    if b.get(pe..pe + 4) != Some(b"PE\0\0") || word(pe + 24)? != 0x10b {
        return Err(bad().into());
    }
    let base = dword(pe + 52)?;
    let sect = pe + 24 + word(pe + 20)? as usize;
    let offset = |va: u32, size: usize| -> Result<usize, io::Error> {
        let r = va.checked_sub(base).ok_or_else(bad)?;
        for j in 0..word(pe + 6)? as usize {
            let p = sect + 40 * j;
            if let Some(rel) = r.checked_sub(dword(p + 12)?) {
                if rel as usize + size <= dword(p + 16)? as usize {
                    let o = dword(p + 20)? as usize + rel as usize;
                    if b.get(o..o + size).is_some() {
                        return Ok(o);
                    }
                }
            }
        }
        Err(bad())
    };
    if args.len() == 4 {
        // PC_SA2/STG06.ini: pathlist RVA D7E4E8. LoopHead and LoopPoint
        // use virtual pointers; only accept the original RailPath callback.
        let list = offset(base + 0xd7e4e8, 19 * 4)?;
        for id in 0..18 {
            let head = offset(dword(list + 4 * id)?, 16)?;
            if dword(head + 12)? != 0x4980c0 || word(head)? != 1 {
                return Err(bad().into());
            }
            let n = word(head + 2)? as usize;
            if !(2..=1024).contains(&n) {
                return Err(bad().into());
            }
            let pts = offset(dword(head + 8)?, 20 * n)?;
            for i in 0..n {
                let at = pts + i * 20;
                let xyz = [8, 12, 16].map(|d| f32::from_bits(dword(at + d).unwrap()));
                if !xyz.iter().all(|v| v.is_finite()) {
                    return Err(bad().into());
                }
                println!("{id}\t{i}\t{}\t{}\t{}", xyz[0], xyz[1], xyz[2]);
            }
        }
        if dword(list + 18 * 4)? != 0 {
            return Err(bad().into());
        }
        return Ok(());
    }
    for line in header.lines() {
        let Some(s) = line.strip_prefix("DataArray(") else {
            continue;
        };
        let Some((s, _)) = s.split_once(");") else {
            continue;
        };
        let f: Vec<_> = s.split(',').map(str::trim).collect();
        if f.len() != 4 {
            continue;
        }
        let n = f[1];
        let stride = if f[0] == "AnimationInfo" && n.ends_with("AnimList") {
            16
        } else if n == "PhysicsArray" && f[0] == "PhysicsData" {
            132
        } else {
            continue;
        };
        let count = f[3].parse::<usize>()?;
        if count > 1024 {
            return Err(bad().into());
        }
        let o = offset(
            u32::from_str_radix(f[2].trim_start_matches("0x"), 16)?,
            count * stride,
        )?;
        for i in 0..count {
            let p = o + i * stride;
            print!("{n}\t{i}");
            if stride == 16 {
                for j in [0, 2, 4, 6] {
                    print!("\t{}", word(p + j)? as i16);
                }
                for j in [8, 12] {
                    print!("\t{}", f32::from_bits(dword(p + j)?));
                }
            } else {
                print!("\t{}", dword(p)?);
                for j in 1..33 {
                    print!("\t{}", f32::from_bits(dword(p + j * 4)?));
                }
            }
            println!();
        }
    }
    Ok(())
}
