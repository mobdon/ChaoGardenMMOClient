//! Retail interactable placements and ordinary foreground/background trigger tasks.
use crate::{
    Error, VerifiedRom,
    native_placements::{RegionLayout, stage_stream},
    native_player::PlayerState,
};

const TABLE: usize = 0xd4c84;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interactable {
    pub x: i32,
    pub y: i32,
    pub kind: u8,
    pub data: [u8; 4],
}

pub struct StageEntities {
    layout: RegionLayout<Interactable>,
    previous_camera: Option<[i32; 2]>,
    active: Vec<usize>,
    new_springs: Vec<usize>,
    new_checkpoints: Vec<usize>,
    palette_loaders: usize,
    pub(crate) dispatch_new_priority_2000: bool,
    spawned: Vec<bool>,
}

impl StageEntities {
    pub fn load(rom: &VerifiedRom<'_>, level_id: u8) -> Result<Self, Error> {
        let layout = RegionLayout::parse::<7>(&stage_stream(rom, TABLE, level_id)?, |x, y, r| {
            Interactable {
                x,
                y,
                kind: r[2],
                data: [r[3], r[4], r[5], r[6]],
            }
        })?;
        Ok(Self::new(layout))
    }
    fn new(layout: RegionLayout<Interactable>) -> Self {
        let spawned = vec![false; layout.records.len()];
        Self {
            layout,
            previous_camera: None,
            active: Vec::new(),
            new_springs: Vec::new(),
            new_checkpoints: Vec::new(),
            palette_loaders: 0,
            dispatch_new_priority_2000: false,
            spawned,
        }
    }
    pub fn placements(&self) -> &[Interactable] {
        &self.layout.records
    }

    pub(crate) fn take_new_springs(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.new_springs)
    }
    pub(crate) fn restore_spring(&mut self, index: usize) {
        self.spawned[index] = false;
    }

    pub(crate) fn take_new_checkpoints(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.new_checkpoints)
    }
    pub(crate) fn restore_checkpoint(&mut self, index: usize) {
        if self.layout.records[index].kind == 43 {
            self.palette_loaders -= 1;
        }
        self.spawned[index] = false;
    }

    /// Camera scan and dispatch of the two ordinary plane-trigger types. The
    /// supported host schedules the existing trigger tasks before player motion.
    pub fn update_layers(
        &mut self,
        player: &mut PlayerState,
        camera_x: i32,
        camera_y: i32,
    ) -> usize {
        let [x, y] = [camera_x, camera_y];
        let full = [
            x.saturating_sub(128),
            y.saturating_sub(128),
            x.saturating_add(368),
            y.saturating_add(288),
        ];
        self.dispatch_new_priority_2000 = !self.active.is_empty() || self.palette_loaders != 0;
        match self.previous_camera {
            Some([px, py]) if x.abs_diff(px) <= 248 && y.abs_diff(py) <= 208 => {
                if x != px {
                    let mut horizontal = full;
                    if x > px {
                        horizontal[0] = px.saturating_add(368);
                    } else {
                        horizontal[2] = px.saturating_sub(128);
                    }
                    self.spawn_in(horizontal, true);
                }
                if y != py {
                    let mut vertical = full;
                    if y > py {
                        vertical[1] = py.saturating_add(288);
                    } else {
                        vertical[3] = py.saturating_sub(128);
                    }
                    self.spawn_in(vertical, true);
                }
            }
            _ => self.spawn_in(full, false),
        }
        self.previous_camera = Some([x, y]);
        // TasksExec caches the manager's first successor before the callback.
        // With no active same-priority follower, new tasks first run next tick.
        if !self.dispatch_new_priority_2000 {
            return 0;
        }
        let mut hits = 0;
        let px = player.q_world_x >> 8;
        let py = player.q_world_y >> 8;
        self.active.retain(|&i| {
            let e = self.layout.records[i];
            if (e.x..=e.x + i32::from(e.data[2]) * 8).contains(&px)
                && (e.y..=e.y + i32::from(e.data[3]) * 8).contains(&py)
            {
                player.layer = (player.layer & !1) | e.kind;
                player.move_state &= !0x0100_0000;
                hits += 1;
            }
            // Native task tests the player before restoring its spawn marker.
            let keep = (-128..=368).contains(&(i64::from(e.x) - i64::from(x)))
                && (-128..=288).contains(&(i64::from(e.y) - i64::from(y)));
            if !keep {
                self.spawned[i] = false;
            }
            keep
        });
        hits
    }

    fn spawn_in(&mut self, rect: [i32; 4], strip: bool) {
        let xmax = (self.layout.columns * 256 - 1) as i32;
        let ymax = (self.layout.rows * 256 - 1) as i32;
        let [left, top, right, bottom] = [
            rect[0].clamp(0, xmax),
            rect[1].clamp(0, ymax),
            rect[2].clamp(0, xmax),
            rect[3].clamp(0, ymax),
        ];
        if strip && (left == right || top == bottom) {
            return;
        }
        // The source region-origin test is strict at the high edge, while
        // the individual entity-origin inclusion test uses inclusive bounds.
        for row in (top / 256)..((bottom + 255) / 256) {
            for col in (left / 256)..((right + 255) / 256) {
                let region = row as usize * self.layout.columns + col as usize;
                for i in self.layout.regions[region].clone() {
                    let e = self.layout.records[i];
                    if (e.kind <= 1 || (3..=12).contains(&e.kind) || matches!(e.kind, 43 | 45))
                        && !self.spawned[i]
                        && (left..=right).contains(&e.x)
                        && (top..=bottom).contains(&e.y)
                    {
                        self.spawned[i] = true;
                        match e.kind {
                            0 | 1 => self.active.push(i),
                            43 | 45 => {
                                if e.kind == 43 {
                                    self.palette_loaders += 1;
                                }
                                self.new_checkpoints.push(i);
                            }
                            _ => self.new_springs.push(i),
                        }
                    }
                }
            }
        }
    }
}
