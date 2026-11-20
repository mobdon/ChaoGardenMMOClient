//! Offline reachability over a `PlayWorld`.
//!
//! This is a verification aid, not a native system: it samples walkable
//! surfaces on an XZ lattice, keeps only positions where a body of the given
//! radius and height fits, and links neighbours that can be walked (small
//! steps) or dropped onto. A route proves the static geometry plus registered
//! object solids admit a path for that body; it does not prove retail timing.
use crate::play_world::{PlayWorld, V3};
use std::collections::{BinaryHeap, HashMap};

#[derive(Debug, Clone, Copy)]
pub struct Body {
    pub radius: f32,
    pub height: f32,
    /// Largest upward step taken without jumping.
    pub step: f32,
    /// Largest upward gain allowed by a jump between neighbouring samples.
    pub jump: f32,
}

#[derive(Debug, Clone)]
pub struct Route {
    pub reached: bool,
    pub nodes_explored: usize,
    pub waypoints: Vec<V3>,
    /// Tag of the link used to reach each waypoint (0 = walk/jump/drop).
    pub tags: Vec<u32>,
    /// Nearest explored node to the target when unreached.
    pub closest: V3,
    pub closest_distance: f32,
}

pub struct NavGrid {
    pub spacing: f32,
    origin: [f32; 2],
    pub nodes: Vec<(i32, i32, f32)>,
    columns: HashMap<(i32, i32), Vec<u32>>,
    surfaces: HashMap<(i32, i32), Vec<f32>>,
    clearance: Option<Box<dyn Fn(V3) -> bool>>,
    links: HashMap<u32, Vec<(u32, f32, u32)>>,
    /// Nodes at or above this index were added with `add_node` and are only
    /// entered through links (or from other added nodes).
    lattice: u32,
}

impl NavGrid {
    pub fn build(world: &PlayWorld, body: Body, spacing: f32) -> Self {
        let origin = [world.min[0], world.min[2]];
        let nx = ((world.max[0] - world.min[0]) / spacing).ceil() as i32;
        let nz = ((world.max[2] - world.min[2]) / spacing).ceil() as i32;
        let mut g = Self {
            spacing,
            origin,
            nodes: vec![],
            columns: HashMap::new(),
            surfaces: HashMap::new(),
            clearance: None,
            links: HashMap::new(),
            lattice: u32::MAX,
        };
        for ix in 0..=nx {
            for iz in 0..=nz {
                let (x, z) = g.pos(ix, iz);
                let mut ys = vec![];
                world.column(x, z, |s| ys.push((s.y, s.normal[1])));
                if ys.is_empty() {
                    continue;
                }
                ys.sort_by(|a, b| a.0.total_cmp(&b.0));
                g.surfaces
                    .insert((ix, iz), ys.iter().map(|s| s.0).collect());
                let mut last = f32::MIN;
                for &(y, ny) in &ys {
                    if ny < crate::play_world::WALKABLE_NORMAL_Y || y - last < 1.0 {
                        continue;
                    }
                    last = y;
                    if fits(world, body, [x, y, z]) {
                        let id = g.nodes.len() as u32;
                        g.nodes.push((ix, iz, y));
                        g.columns.entry((ix, iz)).or_default().push(id);
                    }
                }
            }
        }
        g.lattice = g.nodes.len() as u32;
        let w = world.clone();
        g.clearance = Some(Box::new(move |p: V3| fits(&w, body, p)));
        g
    }

    fn pos(&self, ix: i32, iz: i32) -> (f32, f32) {
        (
            self.origin[0] + ix as f32 * self.spacing,
            self.origin[1] + iz as f32 * self.spacing,
        )
    }

    /// Insert a node (for a destination that does not exist yet, such as a
    /// raised lift top). It joins the lattice column nearest to `p`.
    pub fn add_node(&mut self, p: V3) -> u32 {
        let ix = ((p[0] - self.origin[0]) / self.spacing).round() as i32;
        let iz = ((p[2] - self.origin[1]) / self.spacing).round() as i32;
        let id = self.nodes.len() as u32;
        self.nodes.push((ix, iz, p[1]));
        self.columns.entry((ix, iz)).or_default().push(id);
        id
    }

    /// A directed special link (ride, launch) with a caller tag.
    pub fn add_link(&mut self, from: u32, to: u32, cost: f32, tag: u32) {
        self.links.entry(from).or_default().push((to, cost, tag));
    }

    /// Nodes whose XZ lies within `radius` of `p` and height within `dy`.
    pub fn nodes_near(&self, p: V3, radius: f32, dy: f32) -> Vec<u32> {
        let r = (radius / self.spacing).ceil() as i32 + 1;
        let ix = ((p[0] - self.origin[0]) / self.spacing).round() as i32;
        let iz = ((p[2] - self.origin[1]) / self.spacing).round() as i32;
        let mut out = vec![];
        for dx in -r..=r {
            for dz in -r..=r {
                for &id in self.columns.get(&(ix + dx, iz + dz)).into_iter().flatten() {
                    let q = self.node_pos(id);
                    if (q[0] - p[0]).hypot(q[2] - p[2]) <= radius && (q[1] - p[1]).abs() <= dy {
                        out.push(id);
                    }
                }
            }
        }
        out
    }

    pub fn node_pos(&self, id: u32) -> V3 {
        let (ix, iz, y) = self.nodes[id as usize];
        let (x, z) = self.pos(ix, iz);
        [x, y, z]
    }

    /// Nearest node under or near a point (searching a few cells).
    pub fn nearest(&self, p: V3) -> Option<u32> {
        let ix = ((p[0] - self.origin[0]) / self.spacing).round() as i32;
        let iz = ((p[2] - self.origin[1]) / self.spacing).round() as i32;
        let mut best: Option<(f32, u32)> = None;
        for dx in -3..=3 {
            for dz in -3..=3 {
                for &id in self.columns.get(&(ix + dx, iz + dz)).into_iter().flatten() {
                    let q = self.node_pos(id);
                    let dy = p[1] - q[1];
                    // Prefer surfaces at or a little below the point.
                    let d = (q[0] - p[0]).hypot(q[2] - p[2])
                        + if dy < -2.0 { 1000.0 } else { dy.abs() * 0.5 };
                    if best.is_none_or(|(b, _)| d < b) {
                        best = Some((d, id));
                    }
                }
            }
        }
        best.map(|b| b.1)
    }

    fn neighbours(&self, body: Body, id: u32, out: &mut Vec<(u32, f32)>) {
        let (ix, iz, y) = self.nodes[id as usize];
        out.clear();
        for (dx, dz) in [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ] {
            let key = (ix + dx, iz + dz);
            let Some(ids) = self.columns.get(&key) else {
                continue;
            };
            let h = self.spacing * ((dx * dx + dz * dz) as f32).sqrt();
            for &n in ids {
                if n >= self.lattice && id < self.lattice {
                    continue;
                }
                let ny = self.nodes[n as usize].2;
                let dy = ny - y;
                let ok = if dy.abs() <= body.step {
                    true
                } else if dy > 0.0 {
                    dy <= body.jump
                } else {
                    // A drop is valid when nothing lies between the two heights in
                    // the destination column (so the body falls onto it).
                    self.open_drop(key, ny, y + body.height)
                };
                if ok {
                    out.push((
                        n,
                        h + dy.abs() * 0.25
                            + if dy > body.step { 30.0 } else { 0.0 }
                            + near_limit(body, dy),
                    ));
                }
            }
        }
        // Longer links: jumps across a ledge face or pit to a level or higher
        // landing, and walking off a ledge whose face keeps the lower samples
        // out of the 8-neighbourhood. The straight path at the higher height
        // must be clear; a drop also needs an open landing column.
        let reach = (body.radius * 3.0 / self.spacing).ceil() as i32;
        for dx in -reach..=reach {
            for dz in -reach..=reach {
                if dx.abs() <= 1 && dz.abs() <= 1 {
                    continue;
                }
                let key = (ix + dx, iz + dz);
                let Some(ids) = self.columns.get(&key) else {
                    continue;
                };
                for &n in ids {
                    if n >= self.lattice && id < self.lattice {
                        continue;
                    }
                    let ny = self.nodes[n as usize].2;
                    let dy = ny - y;
                    if dy > body.jump.max(body.step) {
                        continue;
                    }
                    if dy < -body.step && !self.open_drop(key, ny, y + body.height) {
                        continue;
                    }
                    let a = self.node_pos(id);
                    let b = self.node_pos(n);
                    let top = a[1].max(b[1]);
                    let clear = (1..4).all(|k| {
                        let t = k as f32 / 4.0;
                        let p = [a[0] + (b[0] - a[0]) * t, top, a[2] + (b[2] - a[2]) * t];
                        self.world_clear(p)
                    });
                    if clear {
                        let h = self.spacing * ((dx * dx + dz * dz) as f32).sqrt();
                        let jump = if dy > -body.step { 40.0 } else { 0.0 };
                        out.push((n, h + jump + dy.abs() * 0.25 + near_limit(body, dy)));
                    }
                }
            }
        }
    }

    fn open_drop(&self, key: (i32, i32), landing: f32, from_top: f32) -> bool {
        self.surfaces
            .get(&key)
            .is_none_or(|ss| ss.iter().all(|&s| s <= landing + 0.5 || s >= from_top))
    }

    fn world_clear(&self, p: V3) -> bool {
        self.clearance.as_ref().is_none_or(|f| f(p))
    }

    /// A* from the node nearest `start` to any node within `goal_radius`
    /// (horizontal) and `goal_height` (vertical) of `goal`.
    pub fn route(
        &self,
        body: Body,
        start: V3,
        goal: V3,
        goal_radius: f32,
        goal_height: f32,
    ) -> Route {
        let mut r = Route {
            reached: false,
            nodes_explored: 0,
            waypoints: vec![],
            tags: vec![],
            closest: start,
            closest_distance: f32::MAX,
        };
        let Some(s) = self.nearest(start) else {
            return r;
        };
        let hdist = |p: V3| (p[0] - goal[0]).hypot(p[2] - goal[2]);
        let in_goal = |p: V3| hdist(p) <= goal_radius && (goal[1] - p[1]).abs() <= goal_height;
        let mut cost = vec![f32::MAX; self.nodes.len()];
        let mut prev = vec![u32::MAX; self.nodes.len()];
        let mut via = vec![0u32; self.nodes.len()];
        let mut heap = BinaryHeap::new();
        cost[s as usize] = 0.0;
        heap.push(Item(0.0, s));
        let mut buf = vec![];
        let mut found = None;
        while let Some(Item(_, id)) = heap.pop() {
            r.nodes_explored += 1;
            let p = self.node_pos(id);
            let d = hdist(p) + (goal[1] - p[1]).abs();
            if d < r.closest_distance {
                r.closest_distance = d;
                r.closest = p;
            }
            if in_goal(p) {
                found = Some(id);
                break;
            }
            self.neighbours(body, id, &mut buf);
            let special = self.links.get(&id).into_iter().flatten();
            for (n, w, tag) in buf.iter().map(|&(n, w)| (n, w, 0)).chain(special.copied()) {
                let c = cost[id as usize] + w;
                if c < cost[n as usize] {
                    cost[n as usize] = c;
                    prev[n as usize] = id;
                    via[n as usize] = tag;
                    let q = self.node_pos(n);
                    let hq = (q[0] - goal[0]).hypot(q[2] - goal[2]);
                    heap.push(Item(c + hq, n));
                }
            }
        }
        if let Some(mut id) = found {
            r.reached = true;
            let mut path = vec![];
            let mut tags = vec![];
            while id != u32::MAX {
                path.push(self.node_pos(id));
                tags.push(via[id as usize]);
                id = prev[id as usize];
            }
            path.reverse();
            tags.reverse();
            r.waypoints = path;
            r.tags = tags;
        }
        r
    }
}

/// Extra cost for a climb close to the jump limit, so a ride that reaches
/// the same place is preferred over a jump that leaves little margin.
fn near_limit(body: Body, dy: f32) -> f32 {
    if dy > body.jump * 0.8 {
        400.0
    } else {
        0.0
    }
}

fn fits(world: &PlayWorld, body: Body, p: V3) -> bool {
    let center = [p[0], p[1] + body.step * 0.5 + body.radius, p[2]];
    let (corr, _) = world.push_out(center, body.radius, 1);
    if corr != [0.0; 3] {
        return false;
    }
    world
        .ceiling_above([p[0], p[1] + 0.5, p[2]], body.height)
        .is_none()
}

#[derive(PartialEq)]
struct Item(f32, u32);
impl Eq for Item {}
impl PartialOrd for Item {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Item {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        o.0.total_cmp(&self.0).then(o.1.cmp(&self.1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play_world::make_tri;

    fn quad(y: f32, x0: f32, x1: f32, z0: f32, z1: f32) -> Vec<crate::play_world::Tri> {
        let a = [x0, y, z0];
        let b = [x1, y, z0];
        let c = [x1, y, z1];
        let d = [x0, y, z1];
        vec![
            make_tri([a, c, b], 1, 0).unwrap(),
            make_tri([a, d, c], 1, 0).unwrap(),
        ]
    }

    #[test]
    fn drops_but_does_not_climb_ledges() {
        // Upper deck at y=100 for x<0, lower floor at y=0 for x>=0.
        let mut t = quad(100.0, -200.0, 0.0, -50.0, 50.0);
        t.extend(quad(0.0, 0.0, 200.0, -50.0, 50.0));
        let w = PlayWorld::from_tris(t);
        let body = Body {
            radius: 8.0,
            height: 21.0,
            step: 12.0,
            jump: 20.0,
        };
        let g = NavGrid::build(&w, body, 10.0);
        let down = g.route(body, [-150.0, 100.0, 0.0], [150.0, 0.0, 0.0], 20.0, 30.0);
        assert!(down.reached);
        let up = g.route(body, [150.0, 0.0, 0.0], [-150.0, 100.0, 0.0], 20.0, 30.0);
        assert!(!up.reached);
    }
}
