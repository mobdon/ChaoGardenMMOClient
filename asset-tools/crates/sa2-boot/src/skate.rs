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

/// The pitch (radians, about the Chao's side axis x; it faces +z) of the
/// rider's foot turn from its rest, eased so it never passes `cap`
/// (cap * tanh(pitch / cap)): the toe up or down along the way he goes,
/// without the turn to the side or the roll; 0 for a cap of 0.
pub fn foot_pitch(now: &M3, rest: &M3, cap: f32) -> f32 {
    if cap <= 0.0 {
        return 0.0;
    }
    let d = mm(&mm(&A, &mm(&unit(now), &tr(&unit(rest)))), &tr(&A));
    // Where the turn takes the forward axis: a pitch of t about x gives (0, -sin t, cos t).
    let pitch = (-d[1][2]).atan2(d[2][2]);
    cap * (pitch / cap).tanh()
}

/// The side turn (radians, about the Chao's up axis y) of the rider's foot
/// turn from its rest, read from where the turn takes the side axis (it
/// holds when the foot is pitched steeply), eased so it never passes `cap`
/// (cap * tanh(turn / cap)); 0 for a cap of 0. The ankle's small twist.
pub fn foot_twist(now: &M3, rest: &M3, cap: f32) -> f32 {
    if cap <= 0.0 {
        return 0.0;
    }
    let d = mm(&mm(&A, &mm(&unit(now), &tr(&unit(rest)))), &tr(&A));
    // A turn of t about y takes the side axis x to (cos t, 0, -sin t).
    let turn = (-d[2][0]).atan2(d[0][0]);
    cap * (turn / cap).tanh()
}

/// The trailing foot's extra swing back, in degrees (owner's rule: 10 to 25
/// while it stays on the Chao). `behind`: how far the rider's foot is behind
/// its rest place, as a share of his full back reach (under 0: ahead). 0.4 of
/// `max` once it is just behind (eased in over the first seventh of the
/// reach, so it does not snap as the foot passes), rising to `max` at full reach.
pub fn foot_back(behind: f32, max: f32) -> f32 {
    let f = behind.clamp(0.0, 1.0);
    let s = (f * 7.0).clamp(0.0, 1.0);
    let s = s * s * (3.0 - 2.0 * s);
    s * 0.4 * max + f * 0.6 * max
}

/// Owner's rule (mob 2026-11-24: far too much twist in the feet on walk and jog): a foot's turn
/// from its rest in the Chao's axes (it faces +z, y up) with its roll taken out and its side
/// turn eased so it never passes `cap` (cap * tanh(turn / cap); none for a cap of 0), as on the
/// run. The pitch (toe up or down along the way it goes) is kept whole. The side turn is read
/// from the side axis, which holds when the foot is pitched steeply.
pub fn ankle_limit(d: &M3, cap: f32) -> M3 {
    let pitch = (-d[1][2]).atan2(d[2][2]);
    let turn = (-d[2][0]).atan2(d[0][0]);
    let twist = if cap <= 0.0 { 0.0 } else { cap * (turn / cap).tanh() };
    let ((sn, cs), (st, ct)) = (pitch.sin_cos(), twist.sin_cos());
    mm(&[[ct, 0.0, st], [0.0, 1.0, 0.0], [-st, 0.0, ct]], &[[1.0, 0.0, 0.0], [0.0, cs, -sn], [0.0, sn, cs]])
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
    fn foot_pitch_takes_the_toe_up_or_down_only() {
        let id: M3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        assert!(foot_pitch(&id, &id, 0.6).abs() < 1e-6, "rest");
        // The rider (faces -x, side z) tips his foot 20 degrees about his side axis: all of it is pitch.
        let (s, c) = 20f32.to_radians().sin_cos();
        let about_z: M3 = [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]];
        let p = foot_pitch(&about_z, &id, 10.0).to_degrees();
        assert!((p.abs() - 20.0).abs() < 0.1, "{p}");
        // Turned to the side about his up axis only: no pitch.
        let about_y: M3 = [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]];
        assert!(foot_pitch(&about_y, &id, 10.0).abs() < 1e-5);
        // A big kick is eased under the cap.
        let (s, c) = 120f32.to_radians().sin_cos();
        let kick: M3 = [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]];
        let p = foot_pitch(&kick, &id, 35f32.to_radians()).to_degrees().abs();
        assert!(p < 35.0 && p > 30.0, "{p}");
        // A cap of 0 (SA2_FOOT_PITCH=0) is exactly 0, never 0/0.
        assert_eq!(foot_pitch(&id, &id, 0.0), 0.0, "rest, no cap");
        assert_eq!(foot_pitch(&kick, &id, 0.0), 0.0, "pitched, no cap");
    }
    #[test]
    fn ankle_limit_keeps_the_pitch_eases_the_side_turn_and_drops_the_roll() {
        let rot = |axis: usize, deg: f32| -> M3 {
            let (s, c) = deg.to_radians().sin_cos();
            match axis { 0 => [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]], 1 => [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]], _ => [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]] }
        };
        let cap = 6f32.to_radians();
        let id = rot(0, 0.0);
        assert!(angle_between(&ankle_limit(&id, cap), &id) < 1e-3, "rest stays rest");
        // The jog's kick: 90 degrees of pitch with 16 to the side and 6 of roll.
        let d = mm(&rot(1, 16.0), &mm(&rot(0, 90.0), &rot(2, 6.0)));
        let l = ankle_limit(&d, cap);
        let (yaw, roll) = ((-l[2][0]).atan2(l[0][0]).to_degrees(), l[1][0].atan2(l[0][0]).to_degrees());
        assert!(yaw.abs() < 6.0 && yaw.abs() > 4.0, "side turn {yaw}");
        assert!(roll.abs() < 1e-3, "roll {roll}");
        // Pitch alone is untouched, at any size.
        for p in [-58.0, 30.0, 92.0] {
            assert!(angle_between(&ankle_limit(&rot(0, p), cap), &rot(0, p)) < 1e-2, "pitch {p}");
        }
        // A small side turn passes nearly whole; a cap of 0 takes all of it out.
        let small = ankle_limit(&rot(1, 1.0), cap);
        assert!(((-small[2][0]).atan2(small[0][0]).to_degrees() - 1.0).abs() < 0.02);
        assert!(angle_between(&ankle_limit(&rot(1, 16.0), 0.0), &id) < 1e-3);
    }
    #[test]
    fn foot_back_is_ten_to_twenty_five_degrees_behind_only() {
        assert_eq!(foot_back(-0.3, 25.0), 0.0, "ahead");
        assert_eq!(foot_back(0.0, 25.0), 0.0, "level");
        assert!((foot_back(0.15, 25.0) - 12.25).abs() < 0.01, "just behind: 10 and a little");
        assert!((foot_back(1.0, 25.0) - 25.0).abs() < 1e-4 && (foot_back(2.0, 25.0) - 25.0).abs() < 1e-4, "full reach");
        assert!(foot_back(0.02, 25.0) < 1.0, "eased in");
    }
    #[test]
    fn foot_twist_is_the_side_turn_only_and_small() {
        let id: M3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let cap = 6f32.to_radians();
        assert_eq!(foot_twist(&id, &id, cap), 0.0, "rest");
        // The rider turns his foot 3 degrees about his up axis: nearly all of it comes through.
        let (s, c) = 3f32.to_radians().sin_cos();
        let about_y: M3 = [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]];
        let t = foot_twist(&about_y, &id, cap).to_degrees().abs();
        assert!(t > 2.6 && t <= 3.0, "{t}");
        // 40 degrees to the side stays under the cap; a pure pitch gives none; a cap of 0 turns it off.
        let (s, c) = 40f32.to_radians().sin_cos();
        let wide: M3 = [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]];
        assert!(foot_twist(&wide, &id, cap).to_degrees().abs() < 6.0);
        let pitch: M3 = [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]];
        assert!(foot_twist(&pitch, &id, cap).abs() < 1e-5);
        assert_eq!(foot_twist(&wide, &id, 0.0), 0.0);
    }
}
