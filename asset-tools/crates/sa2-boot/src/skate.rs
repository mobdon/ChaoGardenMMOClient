//! Shadow's skate on a Chao wearing his shoes: the web hub's retarget
//! (mmo-play src/shadow-skate.js, rotation only), on the game's own rider.
//! For each mapped Chao node: delta = A * S * R^T * A^T, where S is the
//! rider node's rotation now and R at rest (both in the rider's model
//! space) and A turns the rider's facing (-x) to the Chao's (+z); the delta
//! is weighted, capped, and put on the Chao node's rest world rotation.
type M3 = [[f32; 3]; 3];

/// (Chao node, rider node, weight, cap in degrees): the hub's own table
/// (src/shadow-skate.js TARGETS, SKATE_ROTATION_LIMITS: body 24, head 18,
/// arms and hands 75, feet 50; SKATE_ROTATION_WEIGHTS: head 0.35).
/// Rider nodes: body 4, head 44, arms 37/40 and 56/59, heels 23/13.
pub const MAP: [(usize, usize, f32, f32); 8] = [
    (1, 4, 1.0, 24.0),    // body
    (14, 44, 0.35, 18.0), // head
    (3, 37, 1.0, 75.0),   // left upper arm
    (4, 40, 1.0, 75.0),   // left hand
    (10, 56, 1.0, 75.0),  // right upper arm
    (11, 59, 1.0, 75.0),  // right hand
    (6, 23, 1.0, 50.0),   // left foot
    (13, 13, 1.0, 50.0),  // right foot
];

const A: M3 = [[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]];

pub fn mul(a: &M3, b: &M3) -> M3 { mm(a, b) }
/// Inverse of a rotation that may carry scale: (unit R)^T with 1/scale.
pub fn inv(a: &M3) -> M3 {
    let n: [f32; 3] = std::array::from_fn(|j| (0..3).map(|i| a[i][j] * a[i][j]).sum::<f32>().max(1e-12));
    std::array::from_fn(|i| std::array::from_fn(|j| a[j][i] / n[i]))
}

fn mm(a: &M3, b: &M3) -> M3 { std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum())) }
fn tr(a: &M3) -> M3 { std::array::from_fn(|i| std::array::from_fn(|j| a[j][i])) }

/// Columns normalised: the rotation of a matrix that may carry scale.
pub fn unit(a: &M3) -> M3 {
    let n: [f32; 3] = std::array::from_fn(|j| (0..3).map(|i| a[i][j] * a[i][j]).sum::<f32>().sqrt().max(1e-6));
    std::array::from_fn(|i| std::array::from_fn(|j| a[i][j] / n[j]))
}

/// Rotation by `t` of the angle of `r` (same axis), Rodrigues.
fn scale_angle(r: &M3, f: impl Fn(f32) -> f32) -> M3 {
    let c = ((r[0][0] + r[1][1] + r[2][2] - 1.0) / 2.0).clamp(-1.0, 1.0);
    let ang = c.acos();
    if ang < 1e-5 {
        return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    }
    let s = 2.0 * ang.sin();
    let ax = [(r[2][1] - r[1][2]) / s, (r[0][2] - r[2][0]) / s, (r[1][0] - r[0][1]) / s];
    let l = (ax[0] * ax[0] + ax[1] * ax[1] + ax[2] * ax[2]).sqrt().max(1e-6);
    let [x, y, z] = ax.map(|v| v / l);
    let a = f(ang);
    let (sn, cs) = a.sin_cos();
    let t = 1.0 - cs;
    [
        [t * x * x + cs, t * x * y - sn * z, t * x * z + sn * y],
        [t * x * y + sn * z, t * y * y + cs, t * y * z - sn * x],
        [t * x * z - sn * y, t * y * z + sn * x, t * z * z + cs],
    ]
}

/// A Chao node's weight and cap (degrees) as used: the table's (the hub's
/// limits), or by SA2_SKATE_CAPS (debug, to compare): `0` = the rider's full
/// turn on every node; `feet` = the full turn on the feet (6, 13) only.
pub fn limits(node: usize, weight: f32, cap_deg: f32, mode: &str) -> (f32, f32) {
    match mode {
        "0" => (1.0, 180.0),
        "feet" if node == 6 || node == 13 => (1.0, 180.0),
        _ => (weight, cap_deg),
    }
}

/// A Chao node's weight and cap for the player's own Chao, from the pause
/// Extras switches: the feet (6, 13) always take the rider's full turn; the
/// body (1) and head (14) keep the table's limit only while their switch is
/// on; the arms keep the table's.
pub fn rig_limits(node: usize, weight: f32, cap_deg: f32, limit_body: bool, limit_head: bool) -> (f32, f32) {
    let full = match node {
        6 | 13 => true,
        1 => !limit_body,
        14 => !limit_head,
        _ => false,
    };
    if full { (1.0, 180.0) } else { (weight, cap_deg) }
}

/// The rider node's turn from its rest, in the Chao's facing.
fn rider_turn(now: &M3, rest: &M3) -> M3 {
    // SA2_SKATE_A=-1 (debug): the opposite quarter turn, to check the facing.
    let a = if std::env::var("SA2_SKATE_A").is_ok_and(|v| v == "-1") { tr(&A) } else { A };
    mm(&mm(&a, &mm(&unit(now), &tr(&unit(rest)))), &tr(&a))
}

/// The Chao node's desired world rotation (Chao model space).
pub fn desired(now: &M3, rest: &M3, chao_rest: &M3, weight: f32, cap_deg: f32) -> M3 {
    let d = rider_turn(now, rest);
    let cap = cap_deg.to_radians();
    let d = scale_angle(&d, |a| (a * weight).min(cap));
    mm(&d, chao_rest)
}

/// Degrees between two rotations (either may carry scale).
pub fn angle_between(a: &M3, b: &M3) -> f32 {
    let d = mm(&unit(a), &tr(&unit(b)));
    // atan2 of sine and cosine: acos alone is noisy near 0 (identical rotations read ~0.03).
    let s = ((d[2][1] - d[1][2]).powi(2) + (d[0][2] - d[2][0]).powi(2) + (d[1][0] - d[0][1]).powi(2)).sqrt() / 2.0;
    s.atan2((d[0][0] + d[1][1] + d[2][2] - 1.0) / 2.0).to_degrees()
}

/// For the tracking log, in degrees: the rider node's turn from its rest (in
/// the Chao's facing), the Chao node's turn from its rest, and the angle
/// between those two turns (0 = the Chao node tracks the rider node 1:1).
pub fn track(now: &M3, rest: &M3, chao_now: &M3, chao_rest: &M3) -> (f32, f32, f32) {
    let id: M3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let ds = rider_turn(now, rest);
    let dc = mm(&unit(chao_now), &tr(&unit(chao_rest)));
    (angle_between(&ds, &id), angle_between(&dc, &id), angle_between(&ds, &dc))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rest_pose_keeps_the_chao_rest() {
        let r: M3 = [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        let c: M3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let d = desired(&r, &r, &c, 1.0, 50.0);
        for i in 0..3 { for j in 0..3 { assert!((d[i][j] - c[i][j]).abs() < 1e-5); } }
    }
    #[test]
    fn track_is_zero_when_the_chao_node_has_the_desired_rotation() {
        let rest: M3 = [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
        let now = mm(&scale_angle(&[[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]], |_| 0.5), &rest);
        let chao_rest: M3 = [[0.0, 0.0, 2.0], [0.0, 2.0, 0.0], [-2.0, 0.0, 0.0]]; // carries scale
        let d = desired(&now, &rest, &chao_rest, 1.0, 180.0);
        assert!(angle_between(&d, &d) < 1e-4, "identical rotations read 0");
        let (s, c, e) = track(&now, &rest, &d, &chao_rest);
        assert!((s - 0.5f32.to_degrees()).abs() < 0.01 && (c - s).abs() < 0.01 && e < 0.01, "{s} {c} {e}");
        // Capped at 10 degrees: the Chao node turns 10, the error is the rest.
        let d = desired(&now, &rest, &chao_rest, 1.0, 10.0);
        let (s, c, e) = track(&now, &rest, &d, &chao_rest);
        assert!((c - 10.0).abs() < 0.01 && (e - (s - 10.0)).abs() < 0.01, "{s} {c} {e}");
    }
    #[test]
    fn limit_modes() {
        assert_eq!(limits(14, 0.35, 18.0, ""), (0.35, 18.0));
        assert_eq!(limits(14, 0.35, 18.0, "0"), (1.0, 180.0));
        assert_eq!(limits(14, 0.35, 18.0, "feet"), (0.35, 18.0));
        assert_eq!(limits(6, 1.0, 50.0, "feet"), (1.0, 180.0));
    }
    #[test]
    fn extras_switches_pick_each_parts_limit() {
        for (body, head) in [(true, true), (true, false), (false, true), (false, false)] {
            for &(node, _, w, cap) in MAP.iter() {
                let got = rig_limits(node, w, cap, body, head);
                let full = (1.0, 180.0);
                match node {
                    6 | 13 => assert_eq!(got, full, "feet always copy the rider"),
                    1 => assert_eq!(got, if body { (1.0, 24.0) } else { full }),
                    14 => assert_eq!(got, if head { (0.35, 18.0) } else { full }),
                    _ => assert_eq!(got, (1.0, 75.0), "arms as they are"),
                }
            }
        }
    }
    #[test]
    fn cap_limits_the_turn() {
        let id: M3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let big = scale_angle(&[[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]], |a| a); // 90 deg about z
        let d = desired(&big, &id, &id, 1.0, 18.0);
        let ang = ((d[0][0] + d[1][1] + d[2][2] - 1.0) / 2.0).acos().to_degrees();
        assert!((ang - 18.0).abs() < 0.01, "{ang}");
    }
}
