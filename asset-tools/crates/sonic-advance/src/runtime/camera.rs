use crate::{player::Player, stage::Stage};

/// Single-player SA1 camera task, normal gravity, with the ordinary stage bounds.
#[derive(Clone, Copy)]
pub(super) struct Camera {
    pub x: i32,
    pub y: i32,
    radius_bias: i32,
    vertical_margin: i32,
    shift_y: i32,
}

impl Camera {
    pub fn initial(player: &Player) -> Self {
        Self {
            x: (player.x >> 8) - 120,
            y: (player.y >> 8) - 84,
            radius_bias: player.half_height - 4,
            vertical_margin: 0,
            shift_y: 0,
        }
    }

    pub fn update(&mut self, player: &Player, stage: &Stage<'_>) {
        let right = (stage.width_px() as i32 - 240).max(0);
        let bottom = (stage.height_px() as i32 - 160).max(0);
        self.radius_bias += (player.half_height - 4 - self.radius_bias).clamp(-5, 5);
        self.vertical_margin =
            (self.vertical_margin + if player.grounded { -4 } else { 4 }).clamp(0, 24);
        self.x = follow(self.x, (player.x >> 8) - 120, 8, 16, right);
        self.y = follow(
            self.y,
            (player.y >> 8) + self.shift_y - 80 + self.radius_bias,
            self.vertical_margin,
            12,
            bottom,
        );
    }

    pub fn start_goal_pan(&mut self) {
        self.shift_y = (self.shift_y - 1).max(-56);
    }
}

fn follow(current: i32, target: i32, margin: i32, speed: i32, limit: i32) -> i32 {
    let current = current.clamp(0, (limit - 1).max(0));
    let delta = target - current;
    (current + (delta - delta.clamp(-margin, margin)).clamp(-speed, speed)).clamp(0, limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_dead_band_speed_and_last_pixel_limits() {
        assert_eq!(follow(0, 8, 8, 16, 100), 0);
        assert_eq!(follow(0, 9, 8, 16, 100), 1);
        assert_eq!(follow(32, 8, 8, 16, 100), 16);
        assert_eq!(follow(100, 108, 8, 16, 100), 100);
        assert_eq!(follow(100, 107, 8, 16, 100), 99);
    }
}
