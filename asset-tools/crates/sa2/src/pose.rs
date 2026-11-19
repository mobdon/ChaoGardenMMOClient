//! Native-space transform and linear motion evaluation, pinned to the locally
//! verified SAIO SA3D.Modeling 1.2.2 reference. No mesh retargeting or rescaling.
use crate::model::{Model, Node};
use crate::motion::{Keyframe, Motion, MotionChannel};
use crate::{Error, Result};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// Row arrays, column vectors: p' = matrix * [x,y,z,1], world = parent * local.
pub type Matrix4 = [[f64; 4]; 4];
const MAX_NODES: usize = 65_536;
const MAX_KEYS: usize = 2_000_000;
const SUPPORTED_NODE_FLAGS: u32 = 0x1ff;
const NO_ANIMATE: u32 = 0x40;
const ROTATE_ZXY: u32 = 0x20;
const NODE_BAMS_TO_RADIANS: f64 = std::f64::consts::TAU / 65_536.0;
// SAANIM signed full-range BAMSF uses 65535 units per turn in the verified
// reference, distinct from node transforms' ordinary 65536-unit BAMS.
const MOTION_BAMSF_TO_RADIANS: f64 = std::f64::consts::TAU / 65_535.0;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluatedNode {
    pub offset: u32,
    pub parent_offset: Option<u32>,
    pub attach_offset: u32,
    pub flags: u32,
    pub local_matrix: Matrix4,
    pub world_matrix: Matrix4,
    pub position: [f64; 3],
    pub rotation_radians: [f64; 3],
    pub scale: [f64; 3],
    pub motion_part_index: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pose {
    pub frame: f64,
    pub frame_count: Option<u32>,
    pub nodes: Vec<EvaluatedNode>,
}

#[derive(Clone, Copy)]
struct Transform {
    position: [f64; 3],
    rotation: [f64; 3],
    scale: [f64; 3],
    part: Option<u32>,
}

impl Transform {
    fn rest(node: &Node) -> Self {
        Self {
            position: node.position.map(f64::from),
            rotation: node
                .rotation_bams
                .map(|v| f64::from(v) * NODE_BAMS_TO_RADIANS),
            scale: node.scale.map(f64::from),
            part: None,
        }
    }
}

pub fn identity_matrix() -> Matrix4 {
    [
        [1., 0., 0., 0.],
        [0., 1., 0., 0.],
        [0., 0., 1., 0.],
        [0., 0., 0., 1.],
    ]
}

pub fn multiply_matrices(left: &Matrix4, right: &Matrix4) -> Matrix4 {
    let mut out = [[0.; 4]; 4];
    for (row, values) in out.iter_mut().enumerate() {
        for (column, value) in values.iter_mut().enumerate() {
            *value = (0..4).map(|k| left[row][k] * right[k][column]).sum();
        }
    }
    out
}

pub fn transform_point(matrix: &Matrix4, point: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|row| {
        matrix[row][0] * point[0]
            + matrix[row][1] * point[1]
            + matrix[row][2] * point[2]
            + matrix[row][3]
    })
}

pub fn transform_vector(matrix: &Matrix4, vector: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|row| {
        matrix[row][0] * vector[0] + matrix[row][1] * vector[1] + matrix[row][2] * vector[2]
    })
}

fn local_matrix(value: Transform, flags: u32) -> Result<Matrix4> {
    if !value
        .position
        .iter()
        .chain(&value.rotation)
        .chain(&value.scale)
        .all(|x| x.is_finite())
    {
        return Err(Error::invalid("non-finite native pose transform"));
    }
    let [x, y, z] = value.rotation;
    let (sx, cx) = x.sin_cos();
    let (sy, cy) = y.sin_cos();
    let (sz, cz) = z.sin_cos();
    let rx = [
        [1., 0., 0., 0.],
        [0., cx, -sx, 0.],
        [0., sx, cx, 0.],
        [0., 0., 0., 1.],
    ];
    let ry = [
        [cy, 0., sy, 0.],
        [0., 1., 0., 0.],
        [-sy, 0., cy, 0.],
        [0., 0., 0., 1.],
    ];
    let rz = [
        [cz, -sz, 0., 0.],
        [sz, cz, 0., 0.],
        [0., 0., 1., 0.],
        [0., 0., 0., 1.],
    ];
    // Pinned 1.2.2 source explicitly implements SA2 ZXY for the flag named
    // RotateZYX. Later upstream changed it; do not silently follow main.
    let mut result = if flags & ROTATE_ZXY != 0 {
        multiply_matrices(&multiply_matrices(&ry, &rx), &rz)
    } else {
        multiply_matrices(&multiply_matrices(&rz, &ry), &rx)
    };
    for (row, values) in result.iter_mut().take(3).enumerate() {
        for (column, v) in values.iter_mut().take(3).enumerate() {
            *v *= value.scale[column];
        }
        values[3] = value.position[row];
    }
    Ok(result)
}

/// Validate a single source tree (including root siblings), then return its
/// child-first, sibling-second order independently of the supplied slice order.
fn ordered_nodes(nodes: &[Node]) -> Result<Vec<usize>> {
    if nodes.is_empty() || nodes.len() > MAX_NODES {
        return Err(Error::invalid("native pose node count outside bounds"));
    }
    let mut lookup = BTreeMap::new();
    let mut incoming = BTreeSet::new();
    for (index, node) in nodes.iter().enumerate() {
        if node.offset == 0 || lookup.insert(node.offset, index).is_some() {
            return Err(Error::invalid("null or duplicate native pose node offset"));
        }
        if node.flags & !SUPPORTED_NODE_FLAGS != 0 {
            return Err(Error::invalid(format!(
                "unverified native pose node flags {:#x}",
                node.flags
            )));
        }
    }
    for node in nodes {
        if let Some(parent) = node.parent_offset {
            if !lookup.contains_key(&parent) {
                return Err(Error::invalid("native pose parent is missing"));
            }
        }
        for offset in [node.child_offset, node.sibling_offset] {
            if offset != 0 && (!lookup.contains_key(&offset) || !incoming.insert(offset)) {
                return Err(Error::invalid(
                    "native pose missing or shared hierarchy target",
                ));
            }
        }
    }
    let roots = nodes
        .iter()
        .filter(|n| n.parent_offset.is_none() && !incoming.contains(&n.offset))
        .collect::<Vec<_>>();
    if roots.len() != 1 {
        return Err(Error::invalid("native pose needs one connected root chain"));
    }
    let mut pending = vec![(roots[0].offset, None)];
    let mut visited = BTreeSet::new();
    let mut ordered = Vec::with_capacity(nodes.len());
    while let Some((offset, parent)) = pending.pop() {
        let index = lookup[&offset];
        let node = &nodes[index];
        if !visited.insert(offset) || node.parent_offset != parent {
            return Err(Error::invalid(
                "native pose hierarchy cycle or inconsistent parent",
            ));
        }
        ordered.push(index);
        if node.sibling_offset != 0 {
            pending.push((node.sibling_offset, parent));
        }
        if node.child_offset != 0 {
            pending.push((node.child_offset, Some(offset)));
        }
    }
    if ordered.len() != nodes.len() {
        return Err(Error::invalid(
            "native pose contains orphaned or cyclic nodes",
        ));
    }
    Ok(ordered)
}

fn evaluate(
    nodes: &[Node],
    order: &[usize],
    transforms: &[Transform],
) -> Result<Vec<EvaluatedNode>> {
    let mut result: Vec<EvaluatedNode> = Vec::with_capacity(nodes.len());
    let mut parent_lookup: BTreeMap<u32, usize> = BTreeMap::new();
    for &index in order {
        let node = &nodes[index];
        let value = transforms[index];
        let local = local_matrix(value, node.flags)?;
        let world = if let Some(parent) = node.parent_offset {
            let parent_index = *parent_lookup
                .get(&parent)
                .ok_or_else(|| Error::invalid("native pose parent not evaluated"))?;
            multiply_matrices(&result[parent_index].world_matrix, &local)
        } else {
            local
        };
        if !world.iter().flatten().all(|v| v.is_finite()) {
            return Err(Error::invalid("native pose world matrix overflow"));
        }
        parent_lookup.insert(node.offset, result.len());
        result.push(EvaluatedNode {
            offset: node.offset,
            parent_offset: node.parent_offset,
            attach_offset: node.attach_offset,
            flags: node.flags,
            local_matrix: local,
            world_matrix: world,
            position: value.position,
            rotation_radians: value.rotation,
            scale: value.scale,
            motion_part_index: value.part,
        });
    }
    Ok(result)
}

pub fn evaluate_nodes(nodes: &[Node]) -> Result<Vec<EvaluatedNode>> {
    let order = ordered_nodes(nodes)?;
    let transforms = nodes.iter().map(Transform::rest).collect::<Vec<_>>();
    evaluate(nodes, &order, &transforms)
}

fn key_value(key: &Keyframe, rotation: bool, short: bool) -> Result<[f64; 3]> {
    if rotation {
        if key.integers.len() != 3 || !key.floats.is_empty() {
            return Err(Error::invalid("invalid native rotation key dimensions"));
        }
        if short && key.integers.iter().any(|v| i16::try_from(*v).is_err()) {
            return Err(Error::invalid(
                "short native rotation exceeds signed BAMS16 range",
            ));
        }
        Ok(std::array::from_fn(|i| {
            f64::from(key.integers[i]) * MOTION_BAMSF_TO_RADIANS
        }))
    } else {
        if key.floats.len() != 3
            || !key.integers.is_empty()
            || !key.floats.iter().all(|v| v.is_finite())
        {
            return Err(Error::invalid("invalid native vector key dimensions"));
        }
        Ok(std::array::from_fn(|i| f64::from(key.floats[i])))
    }
}

fn sample_channel(
    channel: &MotionChannel,
    frame: f64,
    frame_count: u32,
    short: bool,
) -> Result<Option<[f64; 3]>> {
    if channel.key_count as usize != channel.keys.len() {
        return Err(Error::invalid(
            "native motion key count differs from stored keys",
        ));
    }
    let rotation = channel.kind == "rotationBams";
    let mut previous = None;
    for key in &channel.keys {
        if key.frame < 0
            || key.frame as u32 > frame_count
            || previous.is_some_and(|p| key.frame <= p)
        {
            return Err(Error::invalid(
                "native pose keys must have strictly increasing bounded frames",
            ));
        }
        key_value(key, rotation, short)?;
        previous = Some(key.frame);
    }
    let Some(first) = channel.keys.first() else {
        return Ok(None);
    };
    let upper = channel
        .keys
        .partition_point(|key| f64::from(key.frame) <= frame);
    if upper == 0 {
        return Ok(Some(key_value(first, rotation, short)?));
    }
    let before = &channel.keys[upper - 1];
    let a = key_value(before, rotation, short)?;
    if upper == channel.keys.len() || frame == f64::from(before.frame) {
        return Ok(Some(a));
    }
    let after = &channel.keys[upper];
    let b = key_value(after, rotation, short)?;
    let weight = (frame - f64::from(before.frame)) / f64::from(after.frame - before.frame);
    // The verified reference interpolates stored Euler components directly.
    // Shortest-arc wrapping would alter intentional full-turn source keys.
    Ok(Some(std::array::from_fn(|i| {
        a[i] * (1. - weight) + b[i] * weight
    })))
}

pub fn evaluate_model_pose(model: &Model, motion: Option<&Motion>, frame: f64) -> Result<Pose> {
    if !frame.is_finite() {
        return Err(Error::invalid("native pose frame must be finite"));
    }
    let order = ordered_nodes(&model.nodes)?;
    if model.nodes[order[0]].offset != model.root_offset {
        return Err(Error::invalid(
            "native model root differs from hierarchy root",
        ));
    }
    let mut transforms = model.nodes.iter().map(Transform::rest).collect::<Vec<_>>();
    let mut sampled_frame = 0.;
    if let Some(motion) = motion {
        if motion.frame_count == 0 || motion.frame_count > 1_000_000 {
            return Err(Error::invalid("native pose motion duration outside bounds"));
        }
        if motion.type_flags == 0 || motion.type_flags & !7 != 0 {
            return Err(Error::invalid("native pose supports transform-only motions; morph/quaternion/other channels remain unverified"));
        }
        if motion.interpolation_and_slots & !15 != 0
            || (motion.interpolation_and_slots & 15) as u32 != motion.type_flags.count_ones()
        {
            return Err(Error::invalid("native pose supports verified linear interpolation and exact descriptor slots only"));
        }
        let animated = order
            .iter()
            .copied()
            .filter(|&i| model.nodes[i].flags & NO_ANIMATE == 0)
            .collect::<Vec<_>>();
        if animated.len() != motion.part_count as usize || motion.parts.len() != animated.len() {
            return Err(Error::invalid(
                "native motion parts do not match animatable source hierarchy",
            ));
        }
        sampled_frame = frame.max(0.);
        let mut key_total = 0usize;
        for (part_index, (&node_index, part)) in animated.iter().zip(&motion.parts).enumerate() {
            if part.index as usize != part_index
                || part.channels.len() != motion.type_flags.count_ones() as usize
            {
                return Err(Error::invalid(
                    "native motion part order or channel count mismatch",
                ));
            }
            let mut seen = 0u16;
            transforms[node_index].part = Some(part.index);
            for channel in &part.channels {
                let bit = match channel.kind.as_str() {
                    "position" => 1,
                    "rotationBams" => 2,
                    "scale" => 4,
                    _ => return Err(Error::invalid("unverified native pose channel kind")),
                };
                if seen & bit != 0 || motion.type_flags & bit == 0 {
                    return Err(Error::invalid(
                        "duplicate or unexpected native motion channel",
                    ));
                }
                seen |= bit;
                key_total = key_total
                    .checked_add(channel.keys.len())
                    .ok_or_else(|| Error::invalid("native pose key total overflow"))?;
                if key_total > MAX_KEYS {
                    return Err(Error::invalid("native pose key total outside bounds"));
                }
                if let Some(value) = sample_channel(
                    channel,
                    sampled_frame,
                    motion.frame_count,
                    motion.short_rotation,
                )? {
                    match bit {
                        1 => transforms[node_index].position = value,
                        2 => transforms[node_index].rotation = value,
                        4 => transforms[node_index].scale = value,
                        _ => unreachable!(),
                    }
                }
            }
        }
    }
    Ok(Pose {
        frame: sampled_frame,
        frame_count: motion.map(|m| m.frame_count),
        nodes: evaluate(&model.nodes, &order, &transforms)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(offset: u32, parent_offset: Option<u32>) -> Node {
        Node {
            offset,
            parent_offset,
            flags: 0,
            attach_offset: 0,
            position: [0.; 3],
            rotation_bams: [0; 3],
            scale: [1.; 3],
            child_offset: 0,
            sibling_offset: 0,
            quaternion_scalar: None,
        }
    }
    fn model(nodes: Vec<Node>) -> Model {
        Model {
            format: "sa2-chunk".into(),
            version: 3,
            root_offset: 16,
            metadata: Vec::new(),
            labels: BTreeMap::new(),
            nodes,
            attachments: Vec::new(),
            unknowns: Vec::new(),
        }
    }
    fn motion(keys: &[(i32, [f32; 3])]) -> Motion {
        Motion {
            version: 2,
            offset: 0,
            frame_count: 10,
            type_flags: 1,
            interpolation_and_slots: 1,
            short_rotation: false,
            part_count: 1,
            labels: BTreeMap::new(),
            metadata: Vec::new(),
            parts: vec![crate::motion::MotionPart {
                index: 0,
                channels: vec![MotionChannel {
                    kind: "position".into(),
                    source_offset: 0,
                    key_count: keys.len() as u32,
                    keys: keys
                        .iter()
                        .map(|(frame, value)| Keyframe {
                            frame: *frame,
                            floats: value.to_vec(),
                            integers: Vec::new(),
                            raw_words: Vec::new(),
                        })
                        .collect(),
                }],
            }],
            unknowns: Vec::new(),
        }
    }
    fn close(actual: [f64; 3], expected: [f64; 3]) {
        for i in 0..3 {
            assert!(
                (actual[i] - expected[i]).abs() < 1e-10,
                "{actual:?} != {expected:?}"
            );
        }
    }
    #[test]
    fn preserves_native_parent_units_and_distinguishes_vectors() {
        let mut root = node(16, None);
        root.position = [2., 3., 4.];
        root.rotation_bams[2] = 16384;
        root.scale = [2., 3., 4.];
        root.child_offset = 68;
        let mut child = node(68, Some(16));
        child.position = [1., 0., 0.];
        let result = evaluate_nodes(&[child, root]).unwrap();
        close(
            transform_point(&result[1].world_matrix, [0.; 3]),
            [2., 5., 4.],
        );
        close(
            transform_vector(&result[0].world_matrix, [1., 0., 0.]),
            [0., 2., 0.],
        );
    }
    #[test]
    fn uses_pinned_sa2_zxy_rotation_variant() {
        let mut a = node(16, None);
        a.rotation_bams = [16384, 16384, 0];
        let regular = evaluate_nodes(&[a.clone()]).unwrap();
        a.flags = ROTATE_ZXY;
        let special = evaluate_nodes(&[a]).unwrap();
        close(
            transform_vector(&regular[0].world_matrix, [0., 0., 1.]),
            [0., -1., 0.],
        );
        close(
            transform_vector(&special[0].world_matrix, [0., 0., 1.]),
            [0., -1., 0.],
        );
        // Noncommuting X and Z distinguish the source ZXY path from XYZ.
        let mut a = node(16, None);
        a.rotation_bams = [16384, 0, 16384];
        let regular = evaluate_nodes(&[a.clone()]).unwrap();
        a.flags = ROTATE_ZXY;
        let special = evaluate_nodes(&[a]).unwrap();
        close(
            transform_vector(&regular[0].world_matrix, [0., 1., 0.]),
            [0., 0., 1.],
        );
        close(
            transform_vector(&special[0].world_matrix, [0., 1., 0.]),
            [-1., 0., 0.],
        );
    }
    #[test]
    fn reference_preserves_authored_values_even_with_identity_hint_flags() {
        let mut a = node(16, None);
        a.flags = 7;
        a.position = [1., 2., 3.];
        a.scale = [2.; 3];
        let pose = evaluate_nodes(&[a]).unwrap();
        close(
            transform_point(&pose[0].world_matrix, [1.; 3]),
            [3., 4., 5.],
        );
    }
    #[test]
    fn samples_linear_and_clamps_endpoints_without_looping() {
        let model = model(vec![node(16, None)]);
        let motion = motion(&[(2, [2., 4., 6.]), (8, [8., 16., 24.])]);
        for (frame, expected) in [
            (-1., [2., 4., 6.]),
            (5., [5., 10., 15.]),
            (100., [8., 16., 24.]),
        ] {
            let pose = evaluate_model_pose(&model, Some(&motion), frame).unwrap();
            close(pose.nodes[0].position, expected);
        }
    }
    #[test]
    fn rejects_unverified_modes_and_bad_keys() {
        let model = model(vec![node(16, None)]);
        let mut m = motion(&[(0, [0.; 3]), (10, [1.; 3])]);
        m.interpolation_and_slots = 0x41;
        assert!(evaluate_model_pose(&model, Some(&m), 0.).is_err());
        m.interpolation_and_slots = 1;
        m.parts[0].channels[0].keys[1].frame = 0;
        assert!(evaluate_model_pose(&model, Some(&m), 0.).is_err());
        assert!(evaluate_model_pose(&model, None, f64::NAN).is_err());
    }
    #[test]
    fn no_animate_filters_motion_parts_without_dropping_hierarchy() {
        let mut root = node(16, None);
        root.flags = NO_ANIMATE;
        root.child_offset = 68;
        let model = model(vec![root, node(68, Some(16))]);
        let motion = motion(&[(0, [1., 2., 3.])]);
        let pose = evaluate_model_pose(&model, Some(&motion), 0.).unwrap();
        assert_eq!(pose.nodes.len(), 2);
        assert_eq!(pose.nodes[0].motion_part_index, None);
        assert_eq!(pose.nodes[1].motion_part_index, Some(0));
    }
    #[test]
    fn motion_bamsf_is_distinct_and_angles_are_not_shortest_arc_wrapped() {
        let model = model(vec![node(16, None)]);
        let mut motion = motion(&[(0, [0.; 3]), (10, [0.; 3])]);
        motion.type_flags = 2;
        motion.short_rotation = true;
        let channel = &mut motion.parts[0].channels[0];
        channel.kind = "rotationBams".into();
        for (key, value) in channel.keys.iter_mut().zip([32_000, -32_000]) {
            key.floats.clear();
            key.integers = vec![value, 0, 0];
        }
        let middle = evaluate_model_pose(&model, Some(&motion), 5.).unwrap();
        close(middle.nodes[0].rotation_radians, [0.; 3]);
        motion.short_rotation = false;
        motion.parts[0].channels[0].keys[0].integers[0] = 65_535;
        let first = evaluate_model_pose(&model, Some(&motion), 0.).unwrap();
        close(
            first.nodes[0].rotation_radians,
            [std::f64::consts::TAU, 0., 0.],
        );
        let mut native = node(16, None);
        native.rotation_bams[0] = 65_536;
        let rest = evaluate_nodes(&[native]).unwrap();
        close(rest[0].rotation_radians, [std::f64::consts::TAU, 0., 0.]);
    }
    #[test]
    fn rejects_cycles_orphans_unknown_flags_and_non_finite_values() {
        let mut root = node(16, None);
        root.child_offset = 16;
        assert!(evaluate_nodes(&[root]).is_err());
        assert!(evaluate_nodes(&[node(16, None), node(68, None)]).is_err());
        let mut root = node(16, None);
        root.flags = 0x400;
        assert!(evaluate_nodes(&[root]).is_err());
        let mut root = node(16, None);
        root.position[0] = f32::INFINITY;
        assert!(evaluate_nodes(&[root]).is_err());
    }
}
