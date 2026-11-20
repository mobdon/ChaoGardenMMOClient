//! What the enemy code asks of the rest of the game: collision results,
//! culling, the players, and the child tasks, effects and sounds it
//! starts. Each method stands for one main.dol routine (or a stage-REL
//! child-task creator) that `enemy_lib` does not port, and is named after
//! it. A session implements them; the tests record them.
use super::{PlayerView, V3};

/// A child task, effect, sound or game call an enemy starts.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// fn_8006B7EC(id, 0, 0, volume, position): a positioned sound.
    Sound { id: i32, volume: i32, pos: V3 },
    /// fn_8006A5F4(id, task, 0, volume): a sound following the task.
    TaskSound { id: i32, volume: i32 },
    /// fn_80069D60(id, twp, a, volume, b, position): a sound tied to the
    /// task's work.
    WorkSound {
        id: i32,
        a: i32,
        volume: i32,
        b: i32,
        pos: V3,
    },
    /// The task's display callback is cleared (task +0x14 = 0).
    Hidden,
    /// A child task the stage REL creates (its creator's REL offset), with
    /// the vectors and numbers it is given.
    Child {
        creator: u32,
        args: Vec<V3>,
        ints: Vec<i32>,
        floats: Vec<f32>,
    },
    /// fn_80015730(player, model, ...): the enemy turns into an object the
    /// player carries. `model` is the REL offset of its model.
    Carried { player: i32, model: u32 },
    /// fn_80039EB0(player, speed, angles, 0): the player is knocked back.
    KnockBack {
        player: i32,
        speed: V3,
        angles: [i32; 3],
    },
    /// fn_8002FB2C(player, a, b, 0): the player reacts to the enemy.
    PlayerHurt { player: i32, a: i32, b: i32 },
    /// fn_80016D10(task, parts, models): the explosion into parts.
    Debris,
    /// fn_80062E0C(points).
    Score { points: i32 },
    /// fn_800164FC's drops (work flags 0x4, 0x8, 0x10). Flag 0x4 starts a
    /// child task (sound 0x8015) whose timer and kind come from rand.
    Drop {
        flags: i16,
        animal: Option<(i16, i8)>,
    },
    /// CCL_ClearInfo(task).
    ClearCollision,
    /// A collision entry's attribute word gets bit 0x10 (it is turned off).
    CollisionOff { entry: u8 },
    /// A collision entry's attribute bit 0x10 is cleared again.
    CollisionOn { entry: u8 },
    /// fn_800068BC(task, r): the collision's range (cwp +8).
    CollisionRange { range: f32 },
    /// SetSpringVelocityP(player, 0, speed, 0) and SE_Call(0x1000, ...):
    /// the player is bounced (E KUMI kind 6).
    Spring { player: i32, speed: f32 },
    /// fn_800BD92C(task, position, 15).
    Burrow { pos: V3 },
    /// CCL_Init(task, rows, count, 3): the collision entries are set up from
    /// the REL rows at this section-5 offset.
    CollisionInit { rows: u32, count: u8 },
    /// The task is freed (FreeTask) or set to die (DeadOut).
    Free,
    /// _rename_SetFlag0x20(task): the SET entry is marked so the enemy is
    /// not placed again (kind 9 when it dies).
    SetEntryDead,
}

pub trait EnemyHost {
    /// The byte at 0x801CC168+0x37. While it is set, enemies skip their
    /// timers, turning, movement and motion.
    fn game_37(&self) -> bool;
    /// The bytes at 0x801CC168+0x39 and +0x42 (players are not tested
    /// while either is 1).
    fn game_39_42(&self) -> (u8, u8);
    /// The word at 0x801CC168+0x7C: a frame counter (fn_80062B38 adds 1
    /// each frame +0x37 is clear; fn_80064278 resets it).
    fn game_7c(&self) -> i32;
    /// The byte at 0x801CC168+0x3C.
    fn game_3c(&self) -> u8 {
        0
    }
    /// GetShadowPos(x, y, z): the ground height below a point, or -1e6.
    fn shadow_pos(&mut self, _pos: V3) -> f32 {
        -1.0e6
    }
    /// fn_80005F88(task, 1): the player (0 or 1) touching collision entry
    /// 1, if any (E KUMI kind 6's spring).
    fn spring_touch(&mut self) -> Option<i32> {
        None
    }
    /// Whether E KUMI's shared display task for this model (stg09D.rel
    /// s6+0x2DC0 for 0xA6A94, s6+0x2DC4 for 0xA8FF8) holds its buffer.
    fn kumi_display_buffer(&mut self, _model: u32) -> bool {
        false
    }
    /// The two player tasks (playertwp[0..2]); the rest are empty.
    fn players(&self) -> [Option<PlayerView>; 2];
    /// The player task's mode byte (twp +0).
    fn player_mode(&self, index: usize) -> i8;
    /// CheckRangeOut(task): the enemy is out of range and is removed.
    fn range_out(&mut self) -> bool;
    /// _rename_CheckFlag0x20(task): the task's set entry is marked dead.
    fn set_entry_dead(&self) -> bool;
    /// CCL_IsHitPlayer(task) != 0: the enemy's collision touches a player.
    fn hit_player(&self) -> bool;
    /// The enemy's first collision entry hit something with a hit-info
    /// word set (cwp +4 bit 0, and the u16 at cwp->+0xA4 non-zero).
    fn bumped(&self) -> bool;
    /// fn_800167E0 when the task's flag 4 (hit by collision) is set: the
    /// hit partner is a player or a player attack (CCL_IsHitPlayer or
    /// fn_80006574 on cwp +0x9C).
    fn hit_by_player(&mut self) -> bool;
    /// fn_800167E0 otherwise: the game's damage hook at 0x803AD7E8, called
    /// with the position (false when unset).
    fn damage_hook(&mut self, pos: V3) -> bool;
    /// fn_80006728(task): the collision's hit flag (cwp +4 bit 0).
    fn collision_hit(&self) -> bool;
    /// fn_8003679C(cwp->+0x9C, hit->+0x9C) == 0 and IsThisTaskPlayer of
    /// the hitting task: Some(player) when a player (0 or 1) hit it.
    fn hit_by_player_index(&mut self) -> Option<i32>;
    /// fn_80035C54(task): the points are doubled.
    fn score_doubled(&mut self) -> bool;
    /// fn_80017C00(twp, work): ground and wall collision (main.dol, land
    /// collision). It may move the task and set work fields.
    fn ground(&mut self, task: &mut super::EnemyTask, work: &mut super::EnemyWork);
    /// The first collision entry's radius (cwp->info[0] +0xC).
    fn collision_radius(&self) -> f32;
    fn set_collision_radius(&mut self, r: f32);
    /// CCL_Entry(task): collision is registered this frame.
    fn collision_entry(&mut self);
    fn event(&mut self, e: Event);
}
