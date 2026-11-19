//! City Escape ObjectBoardCol placement callback.
#[derive(Clone, Copy, Debug)]
pub struct BoardPlayer {
    pub position: [f32; 3],
    pub mode: u8,
}

#[derive(Clone, Debug)]
pub struct BoardTrigger {
    position: [f32; 3],
    radius_parameter: f32,
    ang_x: u16,
    initialized: bool,
    latched: u8,
}

impl BoardTrigger {
    pub fn new(position: [f32; 3], radius_parameter: f32, ang_x: u16) -> Self {
        Self {
            position,
            radius_parameter,
            ang_x,
            initialized: false,
            latched: 0,
        }
    }

    /// Returns (player slot, SetInputP code); the native third argument is zero.
    pub fn tick(
        &mut self,
        players: [Option<BoardPlayer>; 2],
        two_player: bool,
        range_out: bool,
    ) -> Vec<(u8, u8)> {
        let mut calls = Vec::new();
        if range_out {
            return calls;
        }
        if !self.initialized {
            self.initialized = true;
            return calls;
        }
        let odd = self.ang_x & 1 != 0;
        for (slot, player) in players.iter().enumerate() {
            let Some(player) = player else { continue };
            let delta = std::array::from_fn::<_, 3, _>(|i| player.position[i] - self.position[i]);
            let distance = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
            let bit = 1 << slot;
            if distance < self.radius_parameter + 10.0 && self.latched & bit == 0 {
                self.latched |= bit;
                calls.push((slot as u8, if odd { 0x22 } else { 0x0f }));
            }
        }
        if two_player {
            for (slot, player) in players.iter().enumerate() {
                if let Some(player) = player {
                    if odd == (0x4c..=0x52).contains(&player.mode) {
                        self.latched &= !(1 << slot);
                    }
                }
            }
        }
        calls
    }
}
