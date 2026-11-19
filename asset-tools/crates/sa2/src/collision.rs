//! Native Basic land collision baked through original node transforms.
//! This module provides geometry and mathematical ray queries, not retail
//! SA2 player collision response, moving-object simulation or navigation.
use crate::pose;
use crate::stage::{BasicMeshset, BasicPolygon, Stage};
use crate::{Error, Result};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub const SURFACE_SOLID: u32 = 1;
pub const SURFACE_DYNAMIC_COLLISION: u32 = 1 << 27;
pub const SURFACE_VISIBLE: u32 = 1 << 31;
const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct BakeOptions {
    /// One native unit remains one output unit by default. A value such as
    /// 0.1 is an explicit presentation adaptation, never baked into sources.
    pub uniform_scale: f64,
    pub max_output_bytes: usize,
}
impl Default for BakeOptions {
    fn default() -> Self {
        Self {
            uniform_scale: 1.0,
            max_output_bytes: MAX_OUTPUT_BYTES,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GeometryKind {
    BasicCollision,
    ChunkVisual,
    BattleGcVisual,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryDescriptor {
    pub entry_index: usize,
    pub entry_offset: u32,
    pub geometry_kind: GeometryKind,
    pub native_collision: bool,
    pub source_visible: bool,
    pub source_solid: bool,
    pub surface_flags: u32,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TriangleSource {
    pub meshset_offset: u32,
    pub polygon_offset: u32,
    pub polygon_type: u8,
    pub source_triangle_index: usize,
    pub reversed: bool,
    pub material_id: u16,
    /// The original meshset +8 field, which is a source reference rather than
    /// an inferred gameplay response. Per-polygon response is not interpreted.
    pub meshset_attributes_field: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollisionMesh {
    pub entry_index: usize,
    pub entry_offset: u32,
    pub node_offset: u32,
    pub attachment_offset: u32,
    pub surface_flags: u32,
    pub vertices: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
    pub triangle_sources: Vec<TriangleSource>,
    pub degenerate_triangle_count: usize,
    pub bounds: Option<Bounds>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollisionWorld {
    pub source_format: String,
    pub coordinate_space: &'static str,
    pub uniform_scale: f64,
    pub scale_adapted: bool,
    pub entries: Vec<EntryDescriptor>,
    pub meshes: Vec<CollisionMesh>,
    pub bounds: Option<Bounds>,
    pub unknowns: Vec<String>,
}

fn charge(remaining: &mut usize, count: usize, size: usize) -> Result<()> {
    let needed = count
        .checked_mul(size)
        .ok_or_else(|| Error::invalid("collision allocation overflow"))?;
    *remaining = remaining
        .checked_sub(needed)
        .ok_or_else(|| Error::invalid("collision output budget exceeded"))?;
    Ok(())
}
fn finite(v: [f64; 3]) -> bool {
    v.iter().all(|x| x.is_finite())
}
fn subtract(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn length(v: [f64; 3]) -> f64 {
    v[0].hypot(v[1]).hypot(v[2])
}
fn include(bounds: &mut Option<Bounds>, point: [f64; 3]) {
    if let Some(b) = bounds {
        for (i, v) in point.iter().enumerate() {
            b.min[i] = b.min[i].min(*v);
            b.max[i] = b.max[i].max(*v);
        }
    } else {
        *bounds = Some(Bounds {
            min: point,
            max: point,
        });
    }
}

pub(crate) fn triangle_count(mesh: &BasicMeshset, poly: &BasicPolygon) -> Result<usize> {
    match mesh.polygon_type {
        0 if poly.indices.len() == 3 => Ok(1),
        1 if poly.indices.len() == 4 => Ok(2),
        3 if poly.indices.len() >= 3 => Ok(poly.indices.len() - 2),
        2 => Err(Error::invalid(
            "Basic NPoly triangulation is unverified; source remains opaque",
        )),
        _ => Err(Error::invalid("invalid Basic polygon type or corner count")),
    }
}
/// Native quad indices form two triangles sharing edge 1--2. Strip parity
/// toggles for every source triple, including degenerate triples.
pub(crate) fn triangle_indices(mesh: &BasicMeshset, poly: &BasicPolygon, i: usize) -> [u32; 3] {
    let v = &poly.indices;
    match mesh.polygon_type {
        0 => [v[0] as u32, v[1] as u32, v[2] as u32],
        1 if i == 0 => [v[0] as u32, v[1] as u32, v[2] as u32],
        1 => [v[2] as u32, v[1] as u32, v[3] as u32],
        _ if (i % 2 == 1) ^ poly.reversed => [v[i + 1] as u32, v[i] as u32, v[i + 2] as u32],
        _ => [v[i] as u32, v[i + 1] as u32, v[i + 2] as u32],
    }
}

pub fn bake_stage(stage: &Stage, scale: f64) -> Result<CollisionWorld> {
    bake_stage_with_options(
        stage,
        BakeOptions {
            uniform_scale: scale,
            ..Default::default()
        },
    )
}

/// No visual attachment is used to replace missing native collision geometry.
/// All Basic entries are retained, including non-solid and invisible surfaces.
pub fn bake_stage_with_options(stage: &Stage, options: BakeOptions) -> Result<CollisionWorld> {
    if !options.uniform_scale.is_finite() || options.uniform_scale <= 0.0 {
        return Err(Error::invalid(
            "collision uniform scale must be positive and finite",
        ));
    }
    if options.max_output_bytes == 0 || options.max_output_bytes > MAX_OUTPUT_BYTES {
        return Err(Error::invalid("collision budget must be in 1..64 MiB"));
    }
    if !matches!(stage.format.as_str(), "sa2" | "sa2-battle")
        || stage.visual_count < -1
        || stage.visual_count as i64 > stage.entries.len() as i64
    {
        return Err(Error::invalid("unsupported collision stage descriptor"));
    }
    let mut remaining = options.max_output_bytes;
    // Fixed report structure and bounded diagnostic strings are charged even
    // for an empty world, before any variable-size source traversal.
    charge(
        &mut remaining,
        1,
        std::mem::size_of::<CollisionWorld>() + 1024,
    )?;
    charge(
        &mut remaining,
        stage.entries.len(),
        std::mem::size_of::<EntryDescriptor>(),
    )?;
    charge(
        &mut remaining,
        stage.basic_attachments.len(),
        std::mem::size_of::<(u32, usize)>() * 4,
    )?;
    let mut attachment_lookup = BTreeMap::new();
    for (i, attach) in stage.basic_attachments.iter().enumerate() {
        if attachment_lookup.insert(attach.offset, i).is_some() {
            return Err(Error::invalid(
                "duplicate Basic collision attachment offset",
            ));
        }
    }
    let mut entries = Vec::with_capacity(stage.entries.len());
    let mut meshes = Vec::new();
    let mut bounds = None;
    for (i, entry) in stage.entries.iter().enumerate() {
        if entry.index != i {
            return Err(Error::invalid(
                "stage entry indices do not match native table order",
            ));
        }
        let collision = if stage.visual_count < 0 {
            entry.surface_flags & SURFACE_VISIBLE == 0
        } else {
            i >= stage.visual_count as usize
        };
        let kind = if collision {
            GeometryKind::BasicCollision
        } else if stage.format == "sa2-battle" {
            GeometryKind::BattleGcVisual
        } else {
            GeometryKind::ChunkVisual
        };
        let expected_format = match kind {
            GeometryKind::BasicCollision => "basic",
            GeometryKind::ChunkVisual => "chunk",
            GeometryKind::BattleGcVisual => "gc-opaque",
        };
        if entry.geometry_format != expected_format {
            return Err(Error::invalid(
                "native collision entry layout disagrees with geometry format",
            ));
        }
        entries.push(EntryDescriptor {
            entry_index: i,
            entry_offset: entry.offset,
            geometry_kind: kind,
            native_collision: collision,
            source_visible: entry.surface_flags & SURFACE_VISIBLE != 0,
            source_solid: entry.surface_flags & SURFACE_SOLID != 0,
            surface_flags: entry.surface_flags,
        });
        if !collision {
            continue;
        }
        if entry.nodes.is_empty() {
            if entry.root_offset != 0 {
                return Err(Error::invalid(
                    "collision root is missing from node hierarchy",
                ));
            }
            continue;
        }
        // Evaluation has its own bounded working set; reject excess before
        // invoking it, including repeated aliased source hierarchies.
        charge(
            &mut remaining,
            entry.nodes.len(),
            std::mem::size_of::<pose::EvaluatedNode>() * 2,
        )?;
        let evaluated = pose::evaluate_nodes(&entry.nodes)?;
        if evaluated.len() != entry.nodes.len() {
            return Err(Error::invalid("incomplete collision node evaluation"));
        }
        if evaluated
            .first()
            .is_none_or(|n| n.offset != entry.root_offset)
        {
            return Err(Error::invalid(
                "collision root differs from evaluated hierarchy",
            ));
        }
        let mut transforms = BTreeMap::new();
        for item in &evaluated {
            if transforms.insert(item.offset, &item.world_matrix).is_some() {
                return Err(Error::invalid("duplicate evaluated collision node"));
            }
        }
        let mut seen_nodes = BTreeSet::new();
        for node in &entry.nodes {
            if !seen_nodes.insert(node.offset) {
                return Err(Error::invalid("duplicate collision node offset"));
            }
            if node.attach_offset == 0 {
                continue;
            }
            let attach = attachment_lookup
                .get(&node.attach_offset)
                .map(|i| &stage.basic_attachments[*i])
                .ok_or_else(|| Error::invalid("missing native Basic collision attachment"))?;
            charge(&mut remaining, 1, std::mem::size_of::<CollisionMesh>() * 2)?;
            charge(
                &mut remaining,
                attach.vertices.len(),
                std::mem::size_of::<[f64; 3]>(),
            )?;
            let mut triangle_total = 0usize;
            for set in &attach.meshsets {
                for poly in &set.polygons {
                    let count = triangle_count(set, poly)?;
                    if poly
                        .indices
                        .iter()
                        .any(|&j| j as usize >= attach.vertices.len())
                    {
                        return Err(Error::invalid(
                            "collision polygon index outside source vertices",
                        ));
                    }
                    triangle_total = triangle_total
                        .checked_add(count)
                        .ok_or_else(|| Error::invalid("collision triangle count overflow"))?;
                }
            }
            charge(
                &mut remaining,
                triangle_total,
                std::mem::size_of::<[u32; 3]>() + std::mem::size_of::<TriangleSource>(),
            )?;
            let matrix = transforms
                .get(&node.offset)
                .ok_or_else(|| Error::invalid("missing collision node world matrix"))?;
            let mut vertices = Vec::with_capacity(attach.vertices.len());
            let mut mesh_bounds = None;
            for &point in &attach.vertices {
                let p = pose::transform_point(matrix, point.map(f64::from))
                    .map(|v| v * options.uniform_scale);
                if !finite(p) {
                    return Err(Error::invalid("non-finite transformed collision vertex"));
                }
                include(&mut mesh_bounds, p);
                include(&mut bounds, p);
                vertices.push(p);
            }
            let mut triangles = Vec::with_capacity(triangle_total);
            let mut triangle_sources = Vec::with_capacity(triangle_total);
            let mut degenerate = 0;
            for set in &attach.meshsets {
                for poly in &set.polygons {
                    for j in 0..triangle_count(set, poly)? {
                        let indices = triangle_indices(set, poly, j);
                        let normal = cross(
                            subtract(vertices[indices[1] as usize], vertices[indices[0] as usize]),
                            subtract(vertices[indices[2] as usize], vertices[indices[0] as usize]),
                        );
                        if !finite(normal) {
                            return Err(Error::invalid(
                                "transformed collision triangle arithmetic overflow",
                            ));
                        }
                        if normal == [0.0; 3] {
                            degenerate += 1;
                        }
                        triangles.push(indices);
                        triangle_sources.push(TriangleSource {
                            meshset_offset: set.offset,
                            polygon_offset: poly.offset,
                            polygon_type: set.polygon_type,
                            source_triangle_index: j,
                            reversed: poly.reversed,
                            material_id: set.material_id,
                            meshset_attributes_field: set.attributes,
                        });
                    }
                }
            }
            meshes.push(CollisionMesh {
                entry_index: i,
                entry_offset: entry.offset,
                node_offset: node.offset,
                attachment_offset: node.attach_offset,
                surface_flags: entry.surface_flags,
                vertices,
                triangles,
                triangle_sources,
                degenerate_triangle_count: degenerate,
                bounds: mesh_bounds,
            });
        }
    }
    Ok(CollisionWorld {source_format:stage.format.clone(),coordinate_space:if options.uniform_scale==1.0 {"native-sa2-axis-unscaled"} else {"native-sa2-axis-uniform-scale-adaptation"},uniform_scale:options.uniform_scale,scale_adapted:options.uniform_scale!=1.0,entries,meshes,bounds,unknowns:vec!["Native surface responses, character contact/step/slide/gravity, dynamic-object updates and native broadphase remain unimplemented".into(),"Basic NPoly is not triangulated; Battle GC visual meshes are not used as collision substitutes".into(),"Ray queries are generic two-sided mathematical queries against baked source geometry; they do not execute retail collision code".into()]})
}

#[derive(Debug, Clone, Copy)]
pub struct RayFilter {
    pub required_flags: u32,
    pub rejected_flags: u32,
}
impl Default for RayFilter {
    fn default() -> Self {
        Self {
            required_flags: SURFACE_SOLID,
            rejected_flags: SURFACE_DYNAMIC_COLLISION,
        }
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RayHit {
    pub mesh_index: usize,
    pub triangle_index: usize,
    pub entry_index: usize,
    pub node_offset: u32,
    pub source_flags: u32,
    pub distance: f64,
    pub position: [f64; 3],
    /// Geometric normal follows original triangle winding, even for back hits.
    pub normal: [f64; 3],
    pub barycentric: [f64; 3],
    pub front_face: bool,
}

impl CollisionWorld {
    /// Solid, non-dynamic surfaces by default. All source surfaces remain in
    /// the world and can be queried using an explicit different filter.
    pub fn raycast(
        &self,
        origin: [f64; 3],
        direction: [f64; 3],
        max_distance: f64,
    ) -> Result<Option<RayHit>> {
        self.raycast_with_filter(origin, direction, max_distance, RayFilter::default())
    }
    pub fn raycast_with_filter(
        &self,
        origin: [f64; 3],
        direction: [f64; 3],
        max_distance: f64,
        filter: RayFilter,
    ) -> Result<Option<RayHit>> {
        if !finite(origin) || !finite(direction) || !max_distance.is_finite() || max_distance <= 0.0
        {
            return Err(Error::invalid(
                "ray requires finite origin/direction and positive finite max distance",
            ));
        }
        if filter.required_flags & filter.rejected_flags != 0 {
            return Err(Error::invalid(
                "ray filter requires and rejects the same flag",
            ));
        }
        let largest_component = direction.into_iter().map(f64::abs).fold(0.0, f64::max);
        if largest_component == 0.0 {
            return Err(Error::invalid("ray direction is zero"));
        }
        // A subnormal length can lose its diagonal factor; a large one can overflow.
        let scaled = direction.map(|v| v / largest_component);
        let norm = length(scaled);
        let direction = scaled.map(|v| v / norm);
        let mut nearest = None;
        let mut closest = max_distance;
        for (mesh_index, mesh) in self.meshes.iter().enumerate() {
            if mesh.surface_flags & filter.required_flags != filter.required_flags
                || mesh.surface_flags & filter.rejected_flags != 0
            {
                continue;
            }
            for (triangle_index, indices) in mesh.triangles.iter().enumerate() {
                let [a, b, c] = indices.map(|i| {
                    mesh.vertices.get(i as usize).copied().ok_or_else(|| {
                        Error::invalid("baked collision triangle index out of bounds")
                    })
                });
                let (a, b, c) = (a?, b?, c?);
                if !finite(a) || !finite(b) || !finite(c) {
                    return Err(Error::invalid("non-finite baked collision triangle"));
                }
                let e1 = subtract(b, a);
                let e2 = subtract(c, a);
                let normal = cross(e1, e2);
                let normal_length = length(normal);
                if !finite(e1) || !finite(e2) || !normal_length.is_finite() {
                    return Err(Error::invalid("ray triangle arithmetic overflow"));
                }
                if normal_length == 0.0 {
                    continue;
                }
                let p = cross(direction, e2);
                let det = dot(e1, p);
                if !det.is_finite() {
                    return Err(Error::invalid("ray determinant overflow"));
                }
                if det.abs() <= normal_length * 1e-12 {
                    continue;
                }
                let tvec = subtract(origin, a);
                let q = cross(tvec, e1);
                let u = dot(tvec, p) / det;
                let v = dot(direction, q) / det;
                let distance = dot(e2, q) / det;
                if !u.is_finite() || !v.is_finite() || !distance.is_finite() {
                    return Err(Error::invalid("ray intersection arithmetic overflow"));
                }
                if u < -1e-12
                    || v < -1e-12
                    || u + v > 1.0 + 1e-12
                    || distance < 0.0
                    || distance > closest
                {
                    continue;
                }
                if nearest.is_some() && distance == closest {
                    continue;
                }
                let position = std::array::from_fn(|i| origin[i] + direction[i] * distance);
                if !finite(position) {
                    return Err(Error::invalid("ray hit point overflow"));
                }
                closest = distance;
                nearest = Some(RayHit {
                    mesh_index,
                    triangle_index,
                    entry_index: mesh.entry_index,
                    node_offset: mesh.node_offset,
                    source_flags: mesh.surface_flags,
                    distance,
                    position,
                    normal: normal.map(|v| v / normal_length),
                    barycentric: [1.0 - u - v, u, v],
                    front_face: dot(normal, direction) < 0.0,
                });
            }
        }
        Ok(nearest)
    }
}
