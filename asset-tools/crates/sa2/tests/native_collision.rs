//! All fixtures are synthetic; original collision geometry remains owner-local.
use sa2_asset_tools::{
    collision::{
        self, BakeOptions, GeometryKind, RayFilter, SURFACE_DYNAMIC_COLLISION, SURFACE_SOLID,
        SURFACE_VISIBLE,
    },
    model::Node,
    stage::{BasicAttachment, BasicMeshset, BasicPolygon, LandEntry, Stage},
};
use std::collections::BTreeMap;

fn node(offset: u32, attach_offset: u32) -> Node {
    Node {
        offset,
        parent_offset: None,
        flags: 0,
        attach_offset,
        position: [0.0; 3],
        rotation_bams: [0; 3],
        scale: [1.0; 3],
        child_offset: 0,
        sibling_offset: 0,
        quaternion_scalar: None,
    }
}
fn stage(polygon_type: u8, indices: &[u16], reversed: bool) -> Stage {
    Stage {
        format: "sa2-battle".into(),
        version: 3,
        offset: 16,
        visual_count: 0,
        far_clip: 0.0,
        texture_name: None,
        texture_list_offset: 0,
        raw_header_words: vec![],
        labels: BTreeMap::new(),
        metadata: vec![],
        entries: vec![LandEntry {
            index: 0,
            offset: 32,
            bounds_center: [0.0; 3],
            bounds_radius: 2.0,
            root_offset: 48,
            field_14_bits: 0,
            block_bits: 0,
            surface_flags: SURFACE_SOLID,
            geometry_format: "basic".into(),
            nodes: vec![node(48, 100)],
        }],
        basic_attachments: vec![BasicAttachment {
            offset: 100,
            vertex_offset: 140,
            normal_offset: 0,
            mesh_offset: 200,
            material_offset: 0,
            bounds_center: [0.0; 3],
            bounds_radius: 2.0,
            vertices: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 1.0],
            ],
            normals: None,
            meshsets: vec![BasicMeshset {
                offset: 200,
                material_id: 7,
                polygon_type,
                attributes: 0x1234,
                polygon_offset: 224,
                polygon_normals_offset: 0,
                vertex_colors_offset: 0,
                uv_offset: 0,
                polygons: vec![BasicPolygon {
                    offset: 224,
                    reversed,
                    indices: indices.to_vec(),
                }],
                colors_argb: vec![],
                uv_signed: vec![],
            }],
            material_words: vec![],
        }],
        chunk_attachments: vec![],
        unknowns: vec![],
    }
}

#[test]
fn native_triangle_quad_and_strip_winding_are_preserved() {
    let tri = collision::bake_stage(&stage(0, &[0, 1, 2], false), 1.0).unwrap();
    assert_eq!(tri.meshes[0].triangles, [[0, 1, 2]]);
    let quad = collision::bake_stage(&stage(1, &[0, 1, 2, 3], false), 1.0).unwrap();
    assert_eq!(quad.meshes[0].triangles, [[0, 1, 2], [2, 1, 3]]);
    let strip = collision::bake_stage(&stage(3, &[0, 1, 2, 3], false), 1.0).unwrap();
    assert_eq!(strip.meshes[0].triangles, quad.meshes[0].triangles);
    let reverse = collision::bake_stage(&stage(3, &[0, 1, 2, 3], true), 1.0).unwrap();
    assert_eq!(reverse.meshes[0].triangles, [[1, 0, 2], [1, 2, 3]]);
    assert!(reverse.meshes[0].triangle_sources.iter().all(|s| s.reversed
        && s.material_id == 7
        && s.polygon_offset == 224
        && s.meshset_attributes_field == 0x1234));
}

#[test]
fn degenerate_strip_steps_retain_parity_and_provenance() {
    let world = collision::bake_stage(&stage(3, &[0, 0, 1, 2], false), 1.0).unwrap();
    assert_eq!(world.meshes[0].triangles, [[0, 0, 1], [1, 0, 2]]);
    assert_eq!(world.meshes[0].degenerate_triangle_count, 1);
    assert_eq!(world.meshes[0].triangle_sources[1].source_triangle_index, 1);
}

#[test]
fn invisible_native_basic_geometry_is_separate_from_visual_descriptors() {
    let mut s = stage(0, &[0, 1, 2], false);
    s.visual_count = 1;
    let visual = LandEntry {
        index: 0,
        offset: 700,
        bounds_center: [0.0; 3],
        bounds_radius: 1.0,
        root_offset: 900,
        field_14_bits: 0,
        block_bits: 0,
        surface_flags: SURFACE_VISIBLE | SURFACE_SOLID,
        geometry_format: "gc-opaque".into(),
        nodes: vec![node(900, 999)],
    };
    s.entries[0].index = 1;
    s.entries.insert(0, visual);
    let world = collision::bake_stage(&s, 1.0).unwrap();
    assert_eq!(world.entries.len(), 2);
    assert_eq!(world.meshes.len(), 1);
    assert!(!world.entries[0].native_collision);
    assert!(world.entries[0].source_visible);
    assert_eq!(world.entries[0].geometry_kind, GeometryKind::BattleGcVisual);
    assert_eq!(world.meshes[0].entry_index, 1);
    assert_eq!(world.meshes[0].surface_flags, SURFACE_SOLID);
    s.entries[1].geometry_format = "gc-opaque".into();
    assert!(collision::bake_stage(&s, 1.0).is_err());
}

#[test]
fn original_parent_transform_and_explicit_uniform_scale_preserve_positions() {
    let mut s = stage(0, &[0, 1, 2], false);
    let mut parent = node(40, 0);
    parent.position = [10.0, 20.0, 30.0];
    parent.child_offset = 48;
    s.entries[0].nodes[0].parent_offset = Some(40);
    s.entries[0].nodes[0].position = [2.0, 0.0, 0.0];
    s.entries[0].nodes.insert(0, parent);
    s.entries[0].root_offset = 40;
    let native = collision::bake_stage(&s, 1.0).unwrap();
    let adapted = collision::bake_stage(&s, 0.1).unwrap();
    assert_eq!(native.meshes[0].vertices[0], [12.0, 20.0, 30.0]);
    assert!(!native.scale_adapted);
    assert!(adapted.scale_adapted);
    for (n, a) in native.meshes[0]
        .vertices
        .iter()
        .zip(&adapted.meshes[0].vertices)
    {
        for i in 0..3 {
            assert!((n[i] * 0.1 - a[i]).abs() < 1e-14);
        }
    }
}

#[test]
fn ray_is_two_sided_and_reports_original_winding_and_true_world_distance() {
    let world = collision::bake_stage(&stage(0, &[0, 1, 2], false), 1.0).unwrap();
    let above = world
        .raycast([0.25, 2.0, 0.25], [0.0, -4.0, 0.0], 2.0)
        .unwrap()
        .unwrap();
    assert_eq!(above.distance, 2.0);
    assert_eq!(above.position, [0.25, 0.0, 0.25]);
    assert_eq!(above.normal, [0.0, -1.0, 0.0]);
    assert_eq!(above.barycentric, [0.5, 0.25, 0.25]);
    assert!(!above.front_face);
    let below = world
        .raycast([0.25, -2.0, 0.25], [0.0, 1.0, 0.0], 2.0)
        .unwrap()
        .unwrap();
    assert!(below.front_face);
    assert!(world
        .raycast([0.25, 2.0, 0.25], [0.0, -1.0, 0.0], 1.999)
        .unwrap()
        .is_none());
    assert!(world
        .raycast([2.0, 2.0, 2.0], [0.0, -1.0, 0.0], 3.0)
        .unwrap()
        .is_none());
    assert!(world
        .raycast([0.25, 2.0, 0.25], [1.0, 0.0, 0.0], 3.0)
        .unwrap()
        .is_none());
}

#[test]
fn diagonal_ray_distance_is_independent_of_direction_magnitude() {
    let world = collision::bake_stage(&stage(0, &[0, 1, 2], false), 1.0).unwrap();
    let origin = [-0.75, 1.0, 0.25];
    for magnitude in [f64::from_bits(1), 1.0, f64::MAX] {
        let direction = [magnitude, -magnitude, 0.0];
        let hit = world.raycast(origin, direction, 1.5).unwrap().unwrap();
        assert!((hit.distance - std::f64::consts::SQRT_2).abs() < 1e-14);
        for (actual, expected) in hit.position.into_iter().zip([0.25, 0.0, 0.25]) {
            assert!((actual - expected).abs() < 1e-14);
        }
        assert!(world.raycast(origin, direction, 1.25).unwrap().is_none());
    }
}

#[test]
fn dynamic_and_non_solid_surface_queries_require_explicit_filter() {
    let mut s = stage(0, &[0, 1, 2], false);
    s.entries[0].surface_flags = SURFACE_SOLID | SURFACE_DYNAMIC_COLLISION;
    let world = collision::bake_stage(&s, 1.0).unwrap();
    assert!(world
        .raycast([0.25, 2.0, 0.25], [0.0, -1.0, 0.0], 3.0)
        .unwrap()
        .is_none());
    let hit = world
        .raycast_with_filter(
            [0.25, 2.0, 0.25],
            [0.0, -1.0, 0.0],
            3.0,
            RayFilter {
                required_flags: SURFACE_SOLID,
                rejected_flags: 0,
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(hit.source_flags, SURFACE_SOLID | SURFACE_DYNAMIC_COLLISION);
    s.entries[0].surface_flags = 0;
    let world = collision::bake_stage(&s, 1.0).unwrap();
    assert_eq!(world.meshes.len(), 1);
    assert!(world
        .raycast([0.25, 2.0, 0.25], [0.0, -1.0, 0.0], 3.0)
        .unwrap()
        .is_none());
}

#[test]
fn malformed_geometry_nonfinite_queries_and_allocation_amplification_fail() {
    for scale in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(collision::bake_stage(&stage(0, &[0, 1, 2], false), scale).is_err());
    }
    assert!(collision::bake_stage(&stage(2, &[0, 1, 2, 3], false), 1.0).is_err());
    assert!(collision::bake_stage(&stage(0, &[0, 1, 99], false), 1.0).is_err());
    let mut s = stage(0, &[0, 1, 2], false);
    s.basic_attachments.clear();
    assert!(collision::bake_stage(&s, 1.0).is_err());
    let mut s = stage(0, &[0, 1, 2], false);
    s.entries[0].index = 1;
    assert!(collision::bake_stage(&s, 1.0).is_err());
    let s = stage(0, &[0, 1, 2], false);
    assert!(collision::bake_stage_with_options(
        &s,
        BakeOptions {
            uniform_scale: 1.0,
            max_output_bytes: 1
        }
    )
    .is_err());
    let world = collision::bake_stage(&s, 1.0).unwrap();
    for (o, d, max) in [
        ([f64::NAN, 0.0, 0.0], [0.0, -1.0, 0.0], 2.0),
        ([0.0; 3], [0.0; 3], 2.0),
        ([0.0; 3], [0.0, -1.0, 0.0], 0.0),
        ([0.0; 3], [0.0, -1.0, 0.0], f64::INFINITY),
    ] {
        assert!(world.raycast(o, d, max).is_err());
    }
}

#[test]
fn aliased_source_attachments_are_charged_for_every_world_instance() {
    let mut s = stage(0, &[0, 1, 2], false);
    s.basic_attachments[0].vertices.resize(10_000, [0.0; 3]);
    for i in 1..10 {
        let offset = 48 + i as u32 * 512;
        s.entries.push(LandEntry {
            index: i,
            offset: 32 + i as u32 * 32,
            bounds_center: [0.0; 3],
            bounds_radius: 2.0,
            root_offset: offset,
            field_14_bits: 0,
            block_bits: 0,
            surface_flags: SURFACE_SOLID,
            geometry_format: "basic".into(),
            nodes: vec![node(offset, 100)],
        });
    }
    assert!(collision::bake_stage_with_options(
        &s,
        BakeOptions {
            uniform_scale: 1.0,
            max_output_bytes: 1024 * 1024
        }
    )
    .is_err());
}

#[test]
fn nearest_hit_and_equal_distance_ties_follow_native_entry_order() {
    let mut s = stage(0, &[0, 1, 2], false);
    let mut second = node(560, 100);
    second.position[1] = 1.0;
    s.entries.push(LandEntry {
        index: 1,
        offset: 64,
        bounds_center: [0.0; 3],
        bounds_radius: 2.0,
        root_offset: 560,
        field_14_bits: 0,
        block_bits: 0,
        surface_flags: SURFACE_SOLID,
        geometry_format: "basic".into(),
        nodes: vec![second],
    });
    let world = collision::bake_stage(&s, 1.0).unwrap();
    let hit = world
        .raycast([0.25, 2.0, 0.25], [0.0, -1.0, 0.0], 10.0)
        .unwrap()
        .unwrap();
    assert_eq!(hit.entry_index, 1);
    assert_eq!(hit.distance, 1.0);
    s.entries[1].nodes[0].position[1] = 0.0;
    let world = collision::bake_stage(&s, 1.0).unwrap();
    assert_eq!(
        world
            .raycast([0.25, 2.0, 0.25], [0.0, -1.0, 0.0], 10.0)
            .unwrap()
            .unwrap()
            .entry_index,
        0
    );
}
