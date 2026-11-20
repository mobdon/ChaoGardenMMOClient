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

/// True when `p` lies inside the closed volume (odd crossings of an upward ray).
pub fn inside_volume(tris: &[[V3; 3]], p: V3) -> bool {
    // A tiny fixed offset keeps the ray off shared diagonals and edges.
    let p = [p[0] + 1.37e-3, p[1], p[2] + 2.11e-3];
    let mut crossings = 0;
    for t in tris {
        let [a, b, c] = *t;
        let d = (b[2] - c[2]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[2] - c[2]);
        if d.abs() < 1e-9 {
            continue;
        }
        let l1 = ((b[2] - c[2]) * (p[0] - c[0]) + (c[0] - b[0]) * (p[2] - c[2])) / d;
        let l2 = ((c[2] - a[2]) * (p[0] - c[0]) + (a[0] - c[0]) * (p[2] - c[2])) / d;
        let l3 = 1.0 - l1 - l2;
        if l1 < 0.0 || l2 < 0.0 || l3 < 0.0 {
            continue;
        }
        if l1 * a[1] + l2 * b[1] + l3 * c[1] > p[1] {
            crossings += 1;
        }
    }
    crossings % 2 == 1
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

    #[test]
    fn closed_box_contains_center() {
        // Unit cube from 12 triangles.
        let v = |x: f32, y: f32, z: f32| [x, y, z];
        let q = |a: V3, b: V3, c: V3, d: V3| [[a, b, c], [a, c, d]];
        let mut t = vec![];
        t.extend(q(
            v(0., 0., 0.),
            v(1., 0., 0.),
            v(1., 0., 1.),
            v(0., 0., 1.),
        ));
        t.extend(q(
            v(0., 1., 0.),
            v(1., 1., 0.),
            v(1., 1., 1.),
            v(0., 1., 1.),
        ));
        assert!(inside_volume(&t, [0.5, 0.5, 0.5]));
        assert!(!inside_volume(&t, [0.5, 1.5, 0.5]));
        assert!(!inside_volume(&t, [1.5, 0.5, 0.5]));
    }
}
