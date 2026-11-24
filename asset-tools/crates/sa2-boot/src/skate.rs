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
pub fn scale_angle(r: &M3, f: impl Fn(f32) -> f32) -> M3 {
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

/// The Chao node's desired world rotation (Chao model space).
pub fn desired(now: &M3, rest: &M3, chao_rest: &M3, weight: f32, cap_deg: f32) -> M3 {
    // SA2_SKATE_A=-1 (debug): the opposite quarter turn, to check the facing.
    let a = if std::env::var("SA2_SKATE_A").is_ok_and(|v| v == "-1") { tr(&A) } else { A };
    let d = mm(&mm(&a, &mm(&unit(now), &tr(&unit(rest)))), &tr(&a));
    let cap = cap_deg.to_radians();
    let d = scale_angle(&d, |a| (a * weight).min(cap));
    mm(&d, chao_rest)
}

/// Where a Chao foot goes for the rider's foot (the overlay's foot place; MMO
/// adaptation). `his_now` / `his_rest`: the rider's foot node in his root
/// node's space (he faces -x, y up, z to his side). `chao_rest`: the Chao's
/// foot node at rest in its placement (it faces +z, x to its side). Sideways
/// by the two stance widths, so its feet meet on its centre line when his do;
/// lift and swing from the rest place by `leg` (the two leg lengths' ratio).
pub fn foot_target(his_now: [f32; 3], his_rest: [f32; 3], chao_rest: [f32; 3], leg: f32) -> [f32; 3] {
    let side = if his_rest[2].abs() > 1e-3 { chao_rest[0].abs() / his_rest[2].abs() } else { leg };
    [his_now[2] * side, chao_rest[1] + leg * (his_now[1] - his_rest[1]), chao_rest[2] - leg * (his_now[0] - his_rest[0])]
}

/// The extra turn that swings a Chao foot about its own node toward a target
/// (MMO adaptation), on top of the rider's foot turn it already has. `rest`
/// is the way from the node to the foot's middle at rest, `to` the way from
/// the node's rest place to the target; `his_rest` / `his_now` are the
/// rider's leg (hip joint to heel) at rest and now, in the Chao's axes. His
/// own leg swing is already in his foot's turn, so it is taken out and the
/// Chao's swing put in: the foot keeps his ankle's turn and leans to its own
/// target. The Chao's swing is eased so it never passes `max` radians
/// (max * tanh(angle / max)); `share` (0..1) of the result is returned.
pub fn foot_swing(rest: [f32; 3], to: [f32; 3], his_rest: [f32; 3], his_now: [f32; 3], max: f32, share: f32) -> M3 {
    use crate::chao_retarget::rot_between;
    let own = scale_angle(&rot_between(rest, to), |a| max * (a / max).tanh());
    let his = rot_between(his_rest, his_now);
    scale_angle(&mm(&own, &tr(&his)), |a| a * share.clamp(0.0, 1.0))
}

/// Degrees between two rotations (either may carry scale).
pub fn angle_between(a: &M3, b: &M3) -> f32 {
    let d = mm(&unit(a), &tr(&unit(b)));
    ((d[0][0] + d[1][1] + d[2][2] - 1.0) / 2.0).clamp(-1.0, 1.0).acos().to_degrees()
}

/// For the tracking log, in degrees: the rider node's turn from its rest (in
/// the Chao's facing), the Chao node's turn from its rest, and the angle
/// between those two turns (0 = the Chao node tracks the rider node 1:1).
pub fn track(now: &M3, rest: &M3, chao_now: &M3, chao_rest: &M3) -> (f32, f32, f32) {
    let id: M3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let ds = mm(&mm(&A, &mm(&unit(now), &tr(&unit(rest)))), &tr(&A));
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
    #[test]
    fn foot_target_follows_the_riders_foot() {
        // His heel at rest 1.2 to his side, the Chao's 0.5 to its side; leg ratio 0.25.
        let (his_rest, chao_rest) = ([0.1, -4.0, 1.2], [0.5, -0.9, 0.05]);
        assert_eq!(foot_target(his_rest, his_rest, chao_rest, 0.25), chao_rest, "rest on rest");
        // On his centre line, lifted 2 and 4 ahead of its rest (he faces -x): on the Chao's centre line, up 0.5, 1 ahead (+z).
        let t = foot_target([-3.9, -2.0, 0.0], his_rest, chao_rest, 0.25);
        assert!(t[0].abs() < 1e-6 && (t[1] + 0.4).abs() < 1e-6 && (t[2] - 1.05).abs() < 1e-6, "{t:?}");
    }
    #[test]
    fn foot_swing_points_the_foot_at_the_target_within_the_limit() {
        let down = [0.0, -1.0, 0.0];
        let turn = |r: &M3, v: [f32; 3]| -> [f32; 3] { std::array::from_fn(|i| (0..3).map(|k| r[i][k] * v[k]).sum()) };
        // Nothing to do at rest.
        let r = foot_swing(down, down, down, down, 1.2, 1.0);
        assert!((turn(&r, down)[1] + 1.0).abs() < 1e-5);
        // A small swing inward is all but followed (10 degrees eased at a 70 degree limit: 9.9).
        let to = [-(10f32.to_radians().sin()), -(10f32.to_radians().cos()), 0.0];
        let got = turn(&foot_swing(down, to, down, down, 70f32.to_radians(), 1.0), down);
        let ang = (got[0] * to[0] + got[1] * to[1]).clamp(-1.0, 1.0).acos().to_degrees();
        assert!(ang < 0.2, "{ang}");
        // A target above the node never takes the foot past the limit.
        let got = turn(&foot_swing(down, [0.0, 1.0, 0.3], down, down, 70f32.to_radians(), 1.0), down);
        // His own leg swing is not added twice: when the Chao's swing is the same as his, nothing is added.
        let r = foot_swing(down, to, down, to, 3.0, 1.0);
        assert!(angle_between(&r, &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]) < 1.0);
        let from_rest = (-got[1]).clamp(-1.0, 1.0).acos().to_degrees();
        assert!(from_rest < 70.0 && from_rest > 60.0, "{from_rest}");
    }
}
