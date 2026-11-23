//! Sonic's pose on the player Chao: the retarget mob approved in the
//! preview clips (handoffs/chao-retarget/render2.py `pose`, board.py
//! `board_tris`), ported 1:1. New Chao MMO hand code; no game data here.
//!
//! Spaces: Sonic node matrices come in Sonic's MODEL space (his root
//! placement removed), as `sa2-assets character-state-pose` gives them.
//! `to_chao` turns that into the Chao's model space (Sonic faces -x, the
//! Chao +z). Output is one world (model-space) matrix per Chao node.
//!
//! Rules (mob 2026-11-21): full body frame (LEAN 1), full leg follow, arms
//! snap to the Chao's own karate/race arm angles (punches) or copy Sonic 1:1
//! (grind, board). Board: soles flat on the real deck under Sonic's ankles,
//! legs keep their rest shape, head tips back at the neck only if it would
//! cross the deck.

pub type V3 = [f32; 3];
pub type M3 = [[f32; 3]; 3];
/// World rotation (with scale) and translation of one node.
pub type Xf = (M3, V3);

const I3: M3 = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];

fn sub(a: V3, b: V3) -> V3 { [a[0] - b[0], a[1] - b[1], a[2] - b[2]] }
fn add(a: V3, b: V3) -> V3 { [a[0] + b[0], a[1] + b[1], a[2] + b[2]] }
fn mul(a: V3, s: f32) -> V3 { [a[0] * s, a[1] * s, a[2] * s] }
fn dot(a: V3, b: V3) -> f32 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] }
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn unit(v: V3) -> V3 {
    let l = dot(v, v).sqrt();
    if l == 0.0 { v } else { mul(v, 1.0 / l) }
}
fn mm(a: &M3, b: &M3) -> M3 {
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 { for j in 0..3 { r[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum(); } }
    r
}
fn mv(a: &M3, v: V3) -> V3 { [dot(a[0], v), dot(a[1], v), dot(a[2], v)] }
fn tmv(a: &M3, v: V3) -> V3 { std::array::from_fn(|i| (0..3).map(|k| a[k][i] * v[k]).sum()) }
fn tr(a: &M3) -> M3 { std::array::from_fn(|i| std::array::from_fn(|j| a[j][i])) }
fn axis_angle(k: V3, s: f32, c: f32) -> M3 {
    let kk: M3 = [[0., -k[2], k[1]], [k[2], 0., -k[0]], [-k[1], k[0], 0.]];
    let k2 = mm(&kk, &kk);
    std::array::from_fn(|i| std::array::from_fn(|j| I3[i][j] + s * kk[i][j] + (1. - c) * k2[i][j]))
}

/// Smallest rotation taking direction `u` onto `v`.
pub fn rot_between(u: V3, v: V3) -> M3 {
    let (u, v) = (unit(u), unit(v));
    let k = cross(u, v);
    let s = dot(k, k).sqrt();
    if s < 1e-6 { return I3; }
    axis_angle(mul(k, 1.0 / s), s, dot(u, v))
}

/// Sonic faces -x (right = z-), the Chao faces +z (right = x-).
pub fn to_chao(v: V3) -> V3 { [v[2], v[1], -v[0]] }

/// The Chao's rest model: the 40-node tree as the game loads it.
pub struct ChaoRest {
    /// (parent, local position, local rotation with scale)
    pub nodes: Vec<(i32, V3, M3)>,
    /// Mesh vertices per node (node-local); empty for nodes without a mesh
    /// and for the eyelid overlays 19/22.
    pub verts: Vec<Vec<V3>>,
    /// The Chao's own arm directions (body frame) per arm mesh node 3/10,
    /// sampled from its karate 570-605 and run/dash/fly/climb motions
    /// (handoffs/chao-retarget/chao-arm-dirs.json, loaded at run time).
    pub arm_dirs: [Vec<V3>; 2],
}

/// Sonic 62-node ids -> Chao: (pivot node, mesh node, Sonic from, Sonic to).
const LIMBS: [(usize, usize, usize, usize); 5] =
    [(2, 3, 37, 40), (9, 10, 56, 59), (5, 6, 17, 24), (12, 13, 7, 14), (7, 8, 27, 30)];
const ARMS: [usize; 2] = [3, 10];
const LIDS: [usize; 2] = [19, 22];

fn pos(m: &[[f32; 4]; 3]) -> V3 { [m[0][3], m[1][3], m[2][3]] }

impl ChaoRest {
    /// World matrices with `extra[n]` pre-multiplied onto node n's world
    /// rotation (children inherit it; n's own position does not move).
    pub fn worlds(&self, extra: &[Option<M3>]) -> Vec<Xf> {
        let mut w: Vec<Xf> = Vec::with_capacity(self.nodes.len());
        for (i, &(p, lp, lr)) in self.nodes.iter().enumerate() {
            let (pr, pt) = if p < 0 { (I3, [0.0; 3]) } else { w[p as usize] };
            let mut r = mm(&pr, &lr);
            if let Some(e) = extra.get(i).copied().flatten() { r = mm(&e, &r); }
            w.push((r, add(pt, mv(&pr, lp))));
        }
        w
    }

    fn world_verts(&self, w: &[Xf], n: usize) -> impl Iterator<Item = V3> + '_ {
        let (r, t) = w[n];
        self.verts[n].iter().map(move |&v| add(t, mv(&r, v)))
    }

    /// Height of the rest model (its vertices' y range, model units).
    pub fn height(&self) -> f32 {
        let w = self.worlds(&[]);
        let ys: Vec<f32> = (0..self.nodes.len()).flat_map(|n| self.world_verts(&w, n).map(|p| p[1]).collect::<Vec<_>>()).collect();
        ys.iter().copied().fold(f32::MIN, f32::max) - ys.iter().copied().fold(f32::MAX, f32::min)
    }

    fn centroid(&self, w: &[Xf], n: usize) -> V3 {
        let c = self.verts[n].len().max(1) as f32;
        mul(self.world_verts(w, n).fold([0.0; 3], add), 1.0 / c)
    }

    /// render2.py `pose`. `sonic`: 62 node matrices (3x4, model space).
    pub fn pose(&self, sonic: &[[[f32; 4]; 3]], arm_snap: bool, arm_fwd: f32) -> Vec<Xf> {
        let p = |i: usize| pos(&sonic[i]);
        let n = self.nodes.len();
        let mut extra = vec![None; n];
        let body = body_rot(sonic);
        extra[1] = Some(body);
        // mob 2026-11-21: the head follows the rider's head node 1:1 (not
        // rigid on the body, which bobs with the run cycle): node 16 gets the
        // head frame in place of the body frame.
        if n > 16 {
            extra[16] = Some(mm(&head_rot(sonic), &tr(&body)));
        }
        let w1 = self.worlds(&extra);
        for &(piv, mesh, a, b) in &LIMBS {
            let u = sub(self.centroid(&w1, mesh), w1[piv].1);
            let v = to_chao(sub(p(b), p(a)));
            extra[piv] = Some(if ARMS.contains(&mesh) {
                let r1 = &w1[1].0;
                let mut vb = tmv(r1, unit(v));
                vb[2] += arm_fwd;
                let vb = unit(vb);
                let d = if arm_snap {
                    let dirs = &self.arm_dirs[(mesh == 10) as usize];
                    dirs.iter().copied().max_by(|x, y| dot(*x, vb).total_cmp(&dot(*y, vb))).unwrap_or(vb)
                } else { vb };
                rot_between(u, mv(r1, d))
            } else {
                rot_between(u, v)
            });
        }
        self.worlds(&extra)
    }

    /// The Chao's body and head stay as its own rest (upright); only its arms
    /// and legs take the rider's limb directions, measured in the rider's own
    /// body frame (so his run lean does not tip the Chao, his stride and arm
    /// swing still show). For Chao World, where the Chao is the player.
    pub fn pose_upright(&self, sonic: &[[[f32; 4]; 3]], arm_fwd: f32) -> Vec<Xf> {
        let p = |i: usize| pos(&sonic[i]);
        let body = body_rot(sonic);
        let mut extra = vec![None; self.nodes.len()];
        let w1 = self.worlds(&extra);
        for &(piv, mesh, a, b) in &LIMBS {
            let u = sub(self.centroid(&w1, mesh), w1[piv].1);
            let mut v = tmv(&body, unit(to_chao(sub(p(b), p(a)))));
            if ARMS.contains(&mesh) {
                v[2] += arm_fwd;
            }
            extra[piv] = Some(rot_between(u, unit(v)));
        }
        self.worlds(&extra)
    }

    /// board.py `board_tris`: re-seats the posed Chao `w` on the board.
    /// `board`: the board's triangles in Chao space (to_chao of Sonic's
    /// board), `scale`: Chao height / Sonic height. Returns the board
    /// centroid (Chao space, unscaled) that the scaled result is centred on;
    /// `w` and `board` come back in that centred, scaled space.
    pub fn seat_on_board(&self, w: &mut [Xf], sonic: &[[[f32; 4]; 3]], board: &mut [[V3; 3]], scale: f32) -> V3 {
        let nv = (board.len() * 3).max(1) as f32;
        let bc = mul(board.iter().flatten().fold([0.0; 3], |a, &b| add(a, b)), 1.0 / nv);
        for t in board.iter_mut() { for q in t.iter_mut() { *q = mul(sub(*q, bc), scale); } }
        let place = |i: usize| mul(sub(to_chao(pos(&sonic[i])), bc), scale);
        // deck normal = the board mesh's thinnest axis, pointing at the hips
        let mut cov = [[0.0f32; 3]; 3];
        for q in board.iter().flatten() { for i in 0..3 { for j in 0..3 { cov[i][j] += q[i] * q[j]; } } }
        let mut u = smallest_eigvec(cov);
        if dot(u, place(2)) < 0.0 { u = mul(u, -1.0); }
        let top = board.iter().flatten().map(|&q| dot(q, u)).fold(f32::MIN, f32::max);
        let deck = |p: V3| -> V3 {
            let o = add(p, mul(u, 10.0));
            let d = mul(u, -1.0);
            let mut best: Option<f32> = None;
            for &[a, b, c] in board.iter() {
                let (e1, e2) = (sub(b, a), sub(c, a));
                let pp = cross(d, e2);
                let det = dot(e1, pp);
                if det.abs() < 1e-9 { continue; }
                let s = sub(o, a);
                let uu = dot(s, pp) / det;
                let q = cross(s, e1);
                let vv = dot(d, q) / det;
                if uu < 0.0 || vv < 0.0 || uu + vv > 1.0 { continue; }
                let t = dot(e2, q) / det;
                best = Some(best.map_or(t, |b: f32| b.min(t)));
            }
            match best { Some(t) => sub(o, mul(u, t)), None => sub(p, mul(u, dot(p, u) - top)) }
        };
        let spots = [deck(place(24)), deck(place(14))];
        // legs: rest shape in the body frame, tipped from body-up to the deck normal
        let mut e = vec![None; self.nodes.len()];
        e[1] = Some(body_rot(sonic));
        let w0 = self.worlds(&e);
        let rd = rot_between(mv(&w0[1].0, [0., 1., 0.]), u);
        for n in [5, 6, 12, 13] { w[n] = (mm(&rd, &w0[n].0), w0[n].1); }
        let low = |w: &[Xf], m: usize| self.world_verts(w, m).map(|p| dot(p, u)).fold(f32::MAX, f32::min);
        let sole = |w: &[Xf], m: usize| {
            let lo = low(w, m);
            let f: Vec<V3> = self.world_verts(w, m).filter(|&p| dot(p, u) < lo + 0.08).collect();
            mul(f.iter().fold([0.0; 3], |a, &b| add(a, b)), 1.0 / f.len().max(1) as f32)
        };
        let legs = [(5, 6), (12, 13)];
        let mid = |a: V3, b: V3| mul(add(a, b), 0.5);
        let sh = sub(mid(spots[0], spots[1]), mid(sole(w, 6), sole(w, 13)));
        for x in w.iter_mut() { x.1 = add(x.1, sh); }
        for (&(piv, m), &sp) in legs.iter().zip(&spots) {
            let d = mul(u, dot(sub(sp, sole(w, m)), u));
            for n in [piv, m] { w[n].1 = add(w[n].1, d); }
            let lift = mul(u, dot(sp, u) - low(w, m) + 0.01); // lowest sole point just on the deck
            for n in [piv, m] { w[n].1 = add(w[n].1, lift); }
        }
        // head (node 16 subtree) tips back about the neck only while it would cross the deck
        let deckh = (dot(spots[0], u) + dot(spots[1], u)) / 2.0 + 0.02;
        let neck = w[16].1;
        let mut head = vec![false; self.nodes.len()];
        head[16] = true;
        for (i, &(p, ..)) in self.nodes.iter().enumerate() { if p >= 0 && head[p as usize] { head[i] = true; } }
        let (sa, ca) = 2f32.to_radians().sin_cos();
        for _ in 0..40 {
            let lowv = (0..w.len()).filter(|&n| head[n])
                .flat_map(|n| self.world_verts(w, n).collect::<Vec<_>>())
                .min_by(|a, b| dot(*a, u).total_cmp(&dot(*b, u)));
            let Some(lowv) = lowv else { break };
            if dot(lowv, u) >= deckh { break; }
            let r = sub(lowv, neck);
            let r = sub(r, mul(u, dot(r, u)));
            if dot(r, r).sqrt() < 1e-6 { break; }
            let mut rr = axis_angle(unit(cross(unit(r), u)), sa, ca);
            if dot(mv(&rr, r), u) < dot(r, u) { rr = tr(&rr); }
            for n in 0..w.len() {
                if head[n] { w[n] = (mm(&rr, &w[n].0), add(neck, mv(&rr, sub(w[n].1, neck)))); }
            }
        }
        bc
    }
}

/// Body frame from Sonic's spine (2->34) and hips (6->16); rest = identity.
pub fn body_rot(sonic: &[[[f32; 4]; 3]]) -> M3 {
    let p = |i: usize| pos(&sonic[i]);
    let up = unit(to_chao(sub(p(34), p(2))));
    let sd = to_chao(sub(p(16), p(6)));
    let sd = unit(sub(sd, mul(up, dot(sd, up))));
    let fw = cross(sd, up);
    std::array::from_fn(|i| [sd[i], up[i], fw[i]])
}

/// Head frame from the rider's head node 43 to the muzzle 47 (forward) and
/// the pair 51/52 (left/right of the face); rest = identity.
/// unverified: that 47 and 51/52 are the muzzle and the face sides in every
/// 62-node rider (positions in Shadow's Radical Highway dump fit).
pub fn head_rot(sonic: &[[[f32; 4]; 3]]) -> M3 {
    let p = |i: usize| pos(&sonic[i]);
    let fw = unit(to_chao(sub(p(47), p(43))));
    let sd = to_chao(sub(p(51), p(52)));
    let sd = unit(sub(sd, mul(fw, dot(sd, fw))));
    let up = cross(fw, sd);
    std::array::from_fn(|i| [sd[i], up[i], fw[i]])
}

/// Eigenvector of the smallest eigenvalue of a symmetric 3x3 (Jacobi).
fn smallest_eigvec(mut a: M3) -> V3 {
    let mut v = I3;
    for _ in 0..50 {
        let (mut p, mut q, mut big) = (0, 1, 0.0f32);
        for (i, j) in [(0, 1), (0, 2), (1, 2)] { if a[i][j].abs() > big { (p, q, big) = (i, j, a[i][j].abs()); } }
        if big < 1e-12 { break; }
        let th = 0.5 * (2.0 * a[p][q]).atan2(a[q][q] - a[p][p]);
        let (s, c) = th.sin_cos();
        let mut j = I3;
        j[p][p] = c; j[q][q] = c; j[p][q] = s; j[q][p] = -s;
        a = mm(&tr(&j), &mm(&a, &j));
        v = mm(&v, &j);
    }
    let k = (0..3).min_by(|&x, &y| a[x][x].total_cmp(&a[y][y])).unwrap();
    [v[0][k], v[1][k], v[2][k]]
}

/// Ninja node rotation from BAMS angles: X, Y, then Z (Rz*Ry*Rx); with the
/// ZXY flag (0x20) Z, X, then Y (Ry*Rx*Rz). Same as sa2 pose.rs.
pub fn ninja_rot(flags: u32, ang: [i32; 3], scl: V3) -> M3 {
    let r = |axis: usize, b: i32| {
        let (s, c) = (b as f32 * std::f32::consts::TAU / 65536.0).sin_cos();
        match axis { 0 => [[1., 0., 0.], [0., c, -s], [0., s, c]], 1 => [[c, 0., s], [0., 1., 0.], [-s, 0., c]], _ => [[c, -s, 0.], [s, c, 0.], [0., 0., 1.]] }
    };
    let (x, y, z) = (r(0, ang[0]), r(1, ang[1]), r(2, ang[2]));
    let m = if flags & 0x20 != 0 { mm(&mm(&y, &x), &z) } else { mm(&mm(&z, &y), &x) };
    std::array::from_fn(|i| std::array::from_fn(|j| m[i][j] * scl[j]))
}

/// Read the Chao's node tree and vertex lists from a Ninja chunk object
/// in guest memory (`r32` = big-endian guest word). Same walk as chunk.py.
/// unverified: that the GC ChaoMain object at the root the caller passes is
/// the same 40-node tree as the Steam AL_RootObject the preview used.
pub fn read_chao(r32: &dyn Fn(u32) -> u32, root: u32, arm_dirs: [Vec<V3>; 2]) -> ChaoRest {
    let f = |a: u32| f32::from_bits(r32(a));
    let mut rest = ChaoRest { nodes: vec![], verts: vec![], arm_dirs };
    fn walk(rest: &mut ChaoRest, r32: &dyn Fn(u32) -> u32, f: &dyn Fn(u32) -> f32, mut o: u32, parent: i32) {
        while o != 0 {
            let (fl, att) = (r32(o), r32(o + 4));
            let lp = [f(o + 8), f(o + 12), f(o + 16)];
            let ang = [r32(o + 20) as i32, r32(o + 24) as i32, r32(o + 28) as i32];
            let scl = [f(o + 32), f(o + 36), f(o + 40)];
            let i = rest.nodes.len();
            rest.nodes.push((parent, lp, ninja_rot(fl, ang, scl)));
            rest.verts.push(if att == 0 || LIDS.contains(&i) { vec![] } else { vlist(r32, f, att) });
            walk(rest, r32, f, r32(o + 44), i as i32);
            o = r32(o + 48);
        }
    }
    walk(&mut rest, r32, &f, root, -1);
    rest
}

fn vlist(r32: &dyn Fn(u32) -> u32, f: &dyn Fn(u32) -> f32, att: u32) -> Vec<V3> {
    let mut out = vec![];
    let mut vp = r32(att);
    while vp != 0 {
        let hdr = r32(vp);
        if hdr & 0xFF == 0xFF { break; }
        let size = hdr >> 16;
        let cnt = r32(vp + 4) >> 16;
        let stride = if cnt != 0 { (size - 1) * 4 / cnt } else { 0 };
        for k in 0..cnt { let a = vp + 8 + k * stride; out.push([f(a), f(a + 4), f(a + 8)]); }
        vp += 4 + size * 4;
    }
    out
}

/// chao-arm-dirs.json: {"3": [[x,y,z],...], "10": [...]}.
pub fn parse_arm_dirs(json: &str) -> Option<[Vec<V3>; 2]> {
    let mut out = [vec![], vec![]];
    for (key, slot) in [("\"3\"", 0), ("\"10\"", 1)] {
        let s = &json[json.find(key)? + key.len()..];
        let s = &s[s.find('[')? + 1..];
        let mut depth = 1;
        let end = s.char_indices().find(|&(_, ch)| { depth += (ch == '[') as i32 - (ch == ']') as i32; depth == 0 })?.0;
        let nums: Vec<f32> = s[..end].split(|ch: char| !(ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == 'e'))
            .filter(|t| !t.is_empty()).map(|t| t.parse().ok()).collect::<Option<_>>()?;
        out[slot] = nums.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn near(a: V3, b: V3) -> bool { (0..3).all(|i| (a[i] - b[i]).abs() < 1e-4) }

    #[test]
    fn rot_between_maps_u_onto_v() {
        let (u, v) = ([0.3, -1.0, 0.2], [1.0, 0.5, -0.4]);
        assert!(near(mv(&rot_between(u, v), unit(u)), unit(v)));
    }

    #[test]
    fn thinnest_axis_of_a_flat_cloud() {
        let c = [[4.0, 0.3, 0.0], [0.3, 1.0, 0.0], [0.0, 0.0, 0.01]];
        assert!(near(smallest_eigvec(c).map(f32::abs), [0., 0., 1.]));
    }

    #[test]
    fn parses_arm_dirs() {
        let d = parse_arm_dirs(r#"{"3": [[0.1, -0.9, 1e-3], [1, 2, 3]], "10": [[-0.5, 0, 0]]}"#).unwrap();
        assert_eq!(d[0], vec![[0.1, -0.9, 0.001], [1., 2., 3.]]);
        assert_eq!(d[1], vec![[-0.5, 0., 0.]]);
    }

    /// Synthetic two-limb rig: a Sonic arm pointing where the Chao arm
    /// already points leaves the arm alone; pointing elsewhere turns it there.
    #[test]
    fn head_frame_rest_is_identity() {
        let mut s = [[[0.0f32; 4]; 3]; 62];
        let set = |s: &mut [[[f32; 4]; 3]; 62], i: usize, p: V3| for k in 0..3 { s[i][k][3] = p[k]; };
        set(&mut s, 43, [0., 5., 0.]);
        set(&mut s, 47, [-1., 5., 0.]); // muzzle ahead: Sonic -x = chao +z
        set(&mut s, 51, [-1., 5., 0.2]); // left of the face: Sonic +z = chao +x
        set(&mut s, 52, [-1., 5., -0.2]);
        let h = head_rot(&s);
        for i in 0..3 { assert!(near(h[i], I3[i])); }
    }

    #[test]
        fn limb_follows_sonic_direction() {
        let mut nodes = vec![(-1, [0.; 3], I3), (0, [0.; 3], I3)];
        let mut verts = vec![vec![], vec![]];
        for i in 2..16 { nodes.push((1, [0.; 3], I3)); verts.push(vec![[0., -1., 0.]]); let _ = i; }
        let rest = ChaoRest { nodes, verts, arm_dirs: [vec![], vec![]] };
        let mut s = [[[0.0f32; 4]; 3]; 62];
        let set = |s: &mut [[[f32; 4]; 3]; 62], i: usize, p: V3| for k in 0..3 { s[i][k][3] = p[k]; };
        set(&mut s, 34, [0., 1., 0.]); // spine up
        set(&mut s, 16, [0., 0., 1.]); // hips: chao +x
        // left arm (Sonic 37->40) points chao +z = Sonic -x
        set(&mut s, 40, [-1., 0., 0.]);
        let w = rest.pose(&s, false, 0.0);
        assert!(near(mv(&w[2].0, [0., -1., 0.]), [0., 0., 1.]));
    }
}
