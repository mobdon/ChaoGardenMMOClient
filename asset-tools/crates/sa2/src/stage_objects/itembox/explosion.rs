//! `ExpManExec`: the bomb item's explosions. Each explosion is a sphere
//! that grows from the box; while it lasts, main.dol's hit test pointer
//! (GSNE8P 0x803AD7E8, called by other objects) points at [`hit_test`].
//!
//! stg42D.rel section 1: spawn 0x12C18, exec 0x12D50, display 0x12E20,
//! dest 0x12EB0, hit test 0x12F44. The manager task is s6 +0x688; its
//! +0x3C heads a list of 0x18-byte nodes (syCalloc): position, radius
//! (+0xC), maximum (+0x10), next (+0x14). New nodes go at the head.
//!
//! Constants: stg42D section 4 +0x710 (start radius 0.0), +0x73C (maximum
//! 500.0), +0x740 (growth 10.0 a frame).
use super::{Draw, Global, HitTestHook, ItemBoxHost, ItemEvent, World, STG42D};
use crate::enemy_lib::engine::distance;
use crate::stage_objects::host::V3;

/// s4 +0x73C.
pub const MAX_RADIUS: f32 = 500.0;
/// s4 +0x740.
pub const GROWTH: f32 = 10.0;
/// ExpMan's exec callback (stg42D section 1).
pub const EXEC: u32 = 0x12d50;

/// One explosion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sphere {
    pub pos: V3,
    pub radius: f32,
    pub max: f32,
}

/// The manager: its spheres, head first.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Explosions {
    pub spheres: Vec<Sphere>,
}

/// 0x12C18: an explosion at `pos`. The manager task is made first when
/// missing (CreateElementalTask(2, 3, 0x12D50, "ExpManExec")); the first
/// node puts the hit test in 0x803AD7E8 if that is empty.
pub fn spawn(world: &mut World, pos: V3, host: &mut impl ItemBoxHost) {
    if world.explosions.is_none() && host.create_elemental_task(2, 3, EXEC, STG42D.explosion_name) {
        world.explosions = Some(Explosions::default());
    }
    let Some(m) = world.explosions.as_mut() else {
        return;
    };
    if !host.sy_calloc(1, 0x18) {
        return;
    }
    if host.hit_test_hook() == HitTestHook::Empty {
        host.item_event(ItemEvent::SetHitTestHook(HitTestHook::Explosions));
    }
    m.spheres.insert(
        0,
        Sphere {
            pos,
            radius: 0.0,
            max: MAX_RADIUS,
        },
    );
}

/// 0x12D50, one frame: every sphere grows; one past its maximum is set to
/// it and freed. With none left the manager's task goes (DestroyTask; its
/// dest is [`dest`]).
pub fn exec(m: &mut Explosions, host: &mut impl ItemBoxHost) {
    let mut kept = Vec::with_capacity(m.spheres.len());
    for mut s in m.spheres.drain(..) {
        s.radius += GROWTH;
        if s.radius > s.max {
            host.item_event(ItemEvent::SyFree);
        } else {
            kept.push(s);
        }
    }
    m.spheres = kept;
    if m.spheres.is_empty() {
        host.item_event(ItemEvent::DestroyTask);
    }
}

/// 0x12E20: each sphere at its place, turned about Y by lbl_801CC168
/// +0x7C times 0x120, drawn by the stage REL's 0x7684(radius, radius / max).
pub fn display(m: &Explosions, host: &mut impl ItemBoxHost) -> Vec<Draw> {
    let mut d = Vec::new();
    for s in &m.spheres {
        let ratio = s.radius / s.max;
        d.push(Draw::Push);
        d.push(Draw::Translate(s.pos));
        d.push(Draw::RotateY(
            host.global(Global::GameTurn).wrapping_mul(0x120),
        ));
        d.push(Draw::Explosion {
            radius: s.radius,
            ratio,
        });
        d.push(Draw::Pop);
    }
    d
}

/// 0x12EB0: the nodes are freed, the manager slot cleared, and the hit
/// test taken out of 0x803AD7E8 if it is still there.
pub fn dest(world: &mut World, host: &mut impl ItemBoxHost) {
    if let Some(m) = world.explosions.take() {
        for _ in &m.spheres {
            host.item_event(ItemEvent::SyFree);
        }
    }
    if host.hit_test_hook() == HitTestHook::Explosions {
        host.item_event(ItemEvent::SetHitTestHook(HitTestHook::Empty));
    }
}

/// 0x12F44: is `point` inside a sphere (njDistanceP2P(centre, point) <
/// radius)?
pub fn hit_test(world: &World, point: V3) -> bool {
    world
        .explosions
        .as_ref()
        .is_some_and(|m| m.spheres.iter().any(|s| distance(s.pos, point) < s.radius))
}
