//! Native PC PATH_TAG lists and PATH_TABLE points. Addresses refer to the pinned image.
use crate::{model::Reader, object_table, pe::PeImage, Error, Result};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
pub struct PathPoint {
    pub angle_x: i16,
    pub angle_z: i16,
    pub length: f32,
    pub position: [f32; 3],
}
#[derive(Debug, Serialize)]
pub struct Path {
    pub tag_va: u32,
    pub path_type: u8,
    pub header_byte_1: u8,
    pub total_length: f32,
    pub points_va: u32,
    pub callback_va: u32,
    pub points: Vec<PathPoint>,
}
#[derive(Debug, Serialize)]
pub struct PathList {
    pub rva: u32,
    pub explicit_count: Option<usize>,
    pub paths: Vec<Path>,
}

fn read_list(
    file: &[u8],
    image: &PeImage,
    rva: u32,
    count: Option<usize>,
    total_points: &mut usize,
) -> Result<PathList> {
    if count.is_some_and(|n| n > 4096) {
        return Err(Error::invalid("path list exceeds 4096 entries"));
    }
    let r = Reader { data: file };
    let mut paths = Vec::new();
    while Some(paths.len()) != count {
        let entry = rva
            .checked_add((paths.len() * 4) as u32)
            .ok_or_else(|| Error::invalid("path list overflow"))?;
        let tag_va = r.u32(image.rva_to_offset(entry, 4)?)?;
        if tag_va == 0 {
            if count.is_some() {
                return Err(Error::invalid("null entry in counted path list"));
            }
            break;
        }
        if paths.len() >= 4096 {
            return Err(Error::invalid("unterminated path list"));
        }
        let t = image.va_to_offset(tag_va, 16)?;
        let n = usize::from(r.u16(t + 2)?);
        *total_points += n;
        if *total_points > 1_000_000 {
            return Err(Error::invalid("path point budget exceeded"));
        }
        let points_va = r.u32(t + 8)?;
        let mut points = Vec::with_capacity(n);
        if n != 0 {
            let start = image.va_to_offset(points_va, n * 20)?;
            for i in 0..n {
                let p = start + i * 20;
                points.push(PathPoint {
                    angle_x: r.u16(p)? as i16,
                    angle_z: r.u16(p + 2)? as i16,
                    length: r.f32(p + 4)?,
                    position: r.vec3(p + 8)?,
                });
            }
        }
        paths.push(Path {
            tag_va,
            path_type: file[t],
            header_byte_1: file[t + 1],
            total_length: r.f32(t + 4)?,
            points_va,
            callback_va: r.u32(t + 12)?,
            points,
        });
    }
    Ok(PathList {
        rva,
        explicit_count: count,
        paths,
    })
}

pub fn read(file: &[u8], config: &str) -> Result<BTreeMap<String, PathList>> {
    if config.len() > 1024 * 1024 {
        return Err(Error::invalid("path config exceeds 1 MiB"));
    }
    let image = object_table::verify_image(file)?;
    let mut lists = BTreeMap::new();
    let mut total_points = 0;
    let (mut name, mut kind, mut address, mut count) = (None, None, None, None::<&str>);
    for raw in config.lines().chain(std::iter::once("[]")) {
        let line = raw.trim();
        if line.starts_with('[') {
            if let (Some(n), Some("pathlist")) = (name, kind) {
                let rva = u32::from_str_radix(
                    address.ok_or_else(|| Error::invalid("missing path list address"))?,
                    16,
                )
                .map_err(|_| Error::invalid("invalid path list address"))?;
                let explicit_count = count
                    .map(str::parse::<usize>)
                    .transpose()
                    .map_err(|_| Error::invalid("invalid path list count"))?;
                if lists.len() >= 128 || lists.contains_key(n) {
                    return Err(Error::invalid("duplicate or excessive path lists"));
                }
                let paths = read_list(file, &image, rva, explicit_count, &mut total_points)?;
                lists.insert(String::from(n), paths);
            }
            name = Some(
                line.strip_prefix('[')
                    .and_then(|s| s.strip_suffix(']'))
                    .ok_or_else(|| Error::invalid("invalid path config section"))?,
            );
            kind = None;
            address = None;
            count = None;
        } else if let Some((k, v)) = line.split_once('=') {
            match k.trim() {
                "type" => kind = Some(v.trim()),
                "address" => address = Some(v.trim()),
                "count" => count = Some(v.trim()),
                _ => {}
            }
        }
    }
    if lists.is_empty() {
        return Err(Error::invalid("no pathlist sections"));
    }
    Ok(lists)
}
