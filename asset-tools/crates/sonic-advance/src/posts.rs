//! Local single-player checkpoint and stage goal tasks.
use crate::{
    entities::{Placement, StageEntities},
    sprite::{AnimationSource, Hitbox, SpritePlayer},
    Result,
};

const CHECKPOINT: u8 = 7;
const GOAL: u8 = 0;
const EGG_ROCKET: usize = 10;

#[derive(Debug, Clone, Copy)]
pub struct PostInput {
    pub x: i32,
    pub y: i32,
    pub half_height: i32,
    pub camera_x: i32,
    pub camera_y: i32,
    pub clock: u32,
    pub ia_override: bool,
    pub body: Option<Hitbox>,
    pub grounded: bool,
    pub ground_speed_q8: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Checkpoint {
    pub index: i8,
    pub x: i32,
    pub y: i32,
    pub clock: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub struct GoalState {
    pub elapsed: u16,
    /// The native show-results task has requested character state 28.
    pub braking: bool,
    pub results_ready: bool,
}

#[derive(Debug, Clone)]
struct Post {
    placement: Placement,
    sprite: Option<SpritePlayer>,
    /// True only for the first goal that the local player crosses.
    finishing: bool,
}

#[derive(Debug, Clone)]
pub struct StagePosts {
    stage: usize,
    posts: Vec<Post>,
    pub checkpoint: Option<Checkpoint>,
    pub goal: Option<GoalState>,
}

fn inside(px: i32, py: i32, origin_x: i32, origin_y: i32, hitbox: Hitbox) -> bool {
    let x = i64::from(px) - i64::from(origin_x);
    let y = i64::from(py) - i64::from(origin_y);
    (i64::from(hitbox.left)..=i64::from(hitbox.right)).contains(&x)
        && (i64::from(hitbox.top)..=i64::from(hitbox.bottom)).contains(&y)
}

impl StagePosts {
    pub fn new(stage_id: usize, entities: &StageEntities) -> Self {
        let posts = entities
            .interactables
            .records
            .iter()
            .filter(|p| p.kind == CHECKPOINT || (p.kind == GOAL && p.data[1] != 0))
            .cloned()
            .map(|placement| Post {
                placement,
                sprite: None,
                finishing: false,
            })
            .collect();
        Self {
            stage: stage_id,
            posts,
            checkpoint: None,
            goal: None,
        }
    }

    pub fn visible_sprites(&self) -> impl Iterator<Item = (i32, i32, &SpritePlayer)> {
        self.posts.iter().filter_map(|post| {
            let p = &post.placement;
            let visible = if p.kind == CHECKPOINT {
                self.stage != EGG_ROCKET
            } else {
                p.data[0] != 0
            };
            post.sprite
                .as_ref()
                .filter(|_| visible)
                .map(|s| (p.x, p.y, s))
        })
    }

    pub fn tick(&mut self, source: &impl AnimationSource, input: PostInput) -> Result<()> {
        for post in &mut self.posts {
            let p = &post.placement;
            let cam_x = i64::from(p.x) - i64::from(input.camera_x);
            let cam_y = i64::from(p.y) - i64::from(input.camera_y);
            if !post.finishing && (!(-128..=368).contains(&cam_x) || !(-128..=288).contains(&cam_y))
            {
                post.sprite = None;
                continue;
            }
            if post.finishing && self.goal.is_some_and(|g| g.results_ready) {
                continue;
            }
            let index = p.data[0] as i8;
            if post.sprite.is_none() {
                let (animation, variant) = if p.kind == CHECKPOINT {
                    (
                        452,
                        if index <= self.checkpoint.map_or(0, |c| c.index) {
                            2
                        } else {
                            0
                        },
                    )
                } else {
                    (453, 0)
                };
                let mut sprite = SpritePlayer::new(animation, variant);
                sprite.priority = 2;
                sprite.oam_order = 18;
                sprite.tick(source)?;
                post.sprite = Some(sprite);
            }
            let sprite = post.sprite.as_mut().expect("post initialized above");
            if p.kind == CHECKPOINT {
                let reached = if self.stage != EGG_ROCKET {
                    sprite.hitboxes[0].is_some_and(|h| inside(input.x, input.y, p.x, p.y, h))
                } else if index == 1 {
                    input.x > p.x
                } else {
                    i64::from(input.y) + i64::from(input.half_height) < i64::from(p.y) + 4
                };
                if sprite.variant == 0 && reached && !input.ia_override {
                    self.checkpoint = Some(Checkpoint {
                        index,
                        x: p.x,
                        y: p.y.saturating_sub(input.half_height),
                        clock: input.clock,
                    });
                    sprite.variant = 1;
                    if self.stage == EGG_ROCKET || sprite.tick(source)?.ended {
                        sprite.variant = 2;
                    }
                    // Task_CheckpointMain calls Task_Checkpoint1 immediately, then
                    // advances the visible animation again before returning.
                    if self.stage != EGG_ROCKET {
                        sprite.tick(source)?;
                    }
                } else if self.stage != EGG_ROCKET
                    && sprite.tick(source)?.ended
                    && sprite.variant == 1
                {
                    sprite.variant = 2;
                }
                continue;
            }

            let reached = input.x > p.x
                && match self.stage {
                    7 => i64::from(input.y) > i64::from(p.y) - 80,
                    EGG_ROCKET => input
                        .body
                        .is_some_and(|h| inside(p.x, p.y, input.x, input.y, h)),
                    _ => true,
                };
            if reached && sprite.variant == 0 {
                sprite.variant = 1;
                if self.goal.is_none() {
                    self.goal = Some(GoalState::default());
                    post.finishing = true;
                }
            }
            if post.finishing {
                let goal = self.goal.as_mut().expect("finishing task owns goal");
                if goal.elapsed > 120 && (self.stage == EGG_ROCKET || goal.braking) {
                    goal.results_ready = true;
                    post.sprite = None;
                    continue;
                }
                if self.stage != EGG_ROCKET
                    && goal.elapsed > 30
                    && input.grounded
                    && input.ground_speed_q8 >= 576
                {
                    goal.braking = true;
                }
                goal.elapsed = goal.elapsed.wrapping_add(1);
            }
            // Waiting goals display the constructor frame, without advancing it.
            if sprite.variant != 0 && p.data[0] != 0 {
                sprite.tick(source)?;
            }
        }
        Ok(())
    }
}
