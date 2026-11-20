//! Original object collision models and death-zone volumes for live play.
//!
//! Geometry is read from the fingerprinted Steam executable (SA Tools PC_SA2
//! stage INIs give the RVAs). What an object *does* with that geometry is a
//! separate question; see `play_session` for the explicit adaptations.
use crate::{
    collision::{triangle_count, triangle_indices},
    death_zones::DeathZone,
    model::Node,
    object_table,
    play_world::V3,
    pose, Error, Result,
};

/// Local-space triangles of a single-node Basic collision object, with the
/// node's own transform applied.
pub fn read_object_solid(exe: &[u8], rva: u32) -> Result<Vec<[V3; 3]>> {
    let image = object_table::verify_image(exe)?;
    let platform = crate::city_land::win32_basic_object(exe, &image, rva)?;
    let evaluated = pose::evaluate_nodes(std::slice::from_ref(&platform.node))?;
    let m = &evaluated
        .first()
        .ok_or_else(|| Error::invalid("object model has no node"))?
        .world_matrix;
    let a = &platform.attachment;
    let verts: Vec<V3> = a
        .vertices
        .iter()
        .map(|&p| pose::transform_point(m, p.map(f64::from)).map(|c| c as f32))
        .collect();
    let mut out = vec![];
    for set in &a.meshsets {
        for poly in &set.polygons {
            for i in 0..triangle_count(set, poly)? {
                let idx = triangle_indices(set, poly, i);
                let get = |k: u32| {
                    verts
                        .get(k as usize)
                        .copied()
                        .ok_or_else(|| Error::invalid("object polygon index outside vertices"))
                };
                out.push([get(idx[0])?, get(idx[1])?, get(idx[2])?]);
            }
        }
    }
    Ok(out)
}

/// World-space triangles of one death-zone volume using its object transform.
pub fn death_zone_triangles(zone: &DeathZone) -> Result<Vec<[V3; 3]>> {
    let node = Node {
        offset: 1,
        parent_offset: None,
        flags: zone.object_flags,
        attach_offset: 0,
        position: zone.position,
        rotation_bams: zone.angles,
        scale: zone.scale,
        child_offset: 0,
        sibling_offset: 0,
        quaternion_scalar: None,
    };
    let evaluated = pose::evaluate_nodes(std::slice::from_ref(&node))?;
    let m = &evaluated[0].world_matrix;
    let verts: Vec<V3> = zone
        .vertices
        .iter()
        .map(|&p| pose::transform_point(m, p.map(f64::from)).map(|c| c as f32))
        .collect();
    let mut out = vec![];
    for poly in &zone.polygons {
        let v = &poly.indices;
        let tri = |a: u16, b: u16, c: u16| -> Result<[V3; 3]> {
            let g = |k: u16| {
                verts
                    .get(k as usize)
                    .copied()
                    .ok_or_else(|| Error::invalid("death-zone index outside vertices"))
            };
            Ok([g(a)?, g(b)?, g(c)?])
        };
        match zone.mesh_type {
            0 if v.len() == 3 => out.push(tri(v[0], v[1], v[2])?),
            1 if v.len() == 4 => {
                out.push(tri(v[0], v[1], v[2])?);
                out.push(tri(v[2], v[1], v[3])?);
            }
            3 if v.len() >= 3 => {
                for i in 0..v.len() - 2 {
                    out.push(tri(v[i], v[i + 1], v[i + 2])?);
                }
            }
            _ => return Err(Error::invalid("unsupported death-zone polygon")),
        }
    }
    Ok(out)
}

/// Rotate a local point by a SET placement's Y angle (BAMS) and translate it.
/// SET X/Z words are object parameters for most stage objects, so they are
/// deliberately not applied here.
pub fn place_y(p: V3, yaw_bams: u16, at: V3) -> V3 {
    let a = yaw_bams as f32 * std::f32::consts::TAU / 65536.0;
    let (s, c) = a.sin_cos();
    [
        at[0] + p[0] * c + p[2] * s,
        at[1] + p[1],
        at[2] - p[0] * s + p[2] * c,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaw_placement_matches_ninja_bams() {
        let p = place_y([10.0, 1.0, 0.0], 0x4000, [100.0, 0.0, 0.0]);
        assert!(
            (p[0] - 100.0).abs() < 1e-4 && (p[2] + 10.0).abs() < 1e-4,
            "{p:?}"
        );
    }
}

/// One SA2 CollisionData row (0x30 bytes) from a stage module's data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionRow {
    pub kind: u8,
    /// Shape: 0 sphere, 1 cylinder, 2 second cylinder kind, 3 box, ...
    pub form: u8,
    pub push: u8,
    pub damage: u8,
    pub attr: u32,
    pub center: V3,
    /// a, b, c, d sizes (for form 3, the box half-extents are a, b, c).
    pub size: [f32; 4],
    pub angles: [u32; 3],
}

/// Read `count` CollisionData rows at `offset` in section 5 of the
/// catalogued stage module `module_file` (digest-checked).
pub fn collision_rows(
    rel_bytes: &[u8],
    module_file: &str,
    offset: u32,
    count: usize,
) -> Result<Vec<CollisionRow>> {
    let info = crate::stage_land::catalog()
        .iter()
        .find(|i| i.filename == module_file)
        .ok_or_else(|| Error::invalid("stage module is not catalogued"))?;
    if rel_bytes.len() != info.size || crate::manifest::sha256(rel_bytes) != info.sha256 {
        return Err(Error::invalid("unsupported stage module revision"));
    }
    let rel = crate::city_rel::CityRel::read_identified(rel_bytes, info.module)?;
    let d = rel.data(5)?;
    let word = |at: usize| -> Result<u32> {
        d.get(at..at + 4)
            .map(|w| u32::from_be_bytes(w.try_into().unwrap()))
            .ok_or_else(|| Error::invalid("collision row outside section"))
    };
    let float = |at: usize| word(at).map(f32::from_bits);
    (0..count)
        .map(|k| {
            let b = offset as usize + 0x30 * k;
            let head = word(b)?.to_be_bytes();
            Ok(CollisionRow {
                kind: head[0],
                form: head[1],
                push: head[2],
                damage: head[3],
                attr: word(b + 4)?,
                center: [float(b + 8)?, float(b + 12)?, float(b + 16)?],
                size: [
                    float(b + 20)?,
                    float(b + 24)?,
                    float(b + 28)?,
                    float(b + 32)?,
                ],
                angles: [word(b + 36)?, word(b + 40)?, word(b + 44)?],
            })
        })
        .collect()
}
