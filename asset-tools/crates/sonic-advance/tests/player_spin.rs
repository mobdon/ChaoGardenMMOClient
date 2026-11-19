use sonic_advance_assets::{
    binary,
    player::{Input, Player},
    sprite::{SpriteBank, SpritePlayer},
    stage::Stage,
};

#[test]
#[ignore = "requires local ASOE image and compares with an independently captured retail run"]
fn crouch_charge_release_roll_brake_and_rolling_jump_match_300_retail_frames() {
    let digest = replay(300, |f| {
        let down = (0..28).contains(&f) || (160..175).contains(&f);
        let input = Input {
            left: (36..56).contains(&f),
            right: (140..170).contains(&f),
            jump: (4..6).contains(&f)
                || (10..12).contains(&f)
                || (14..16).contains(&f)
                || (65..75).contains(&f),
        };
        (input, down)
    });
    assert_eq!(
        digest,
        "a81b4bb767f5f0525f3b7d7339133709564b4fb6e249382d87b2654aeaa1315c"
    );
}

#[test]
#[ignore = "requires exact local ASOE image"]
fn rolling_to_stop_and_rapid_mash_release_match_retail() {
    let digest = replay(64, |f| {
        (
            Input {
                right: f < 8,
                jump: (42..=62).contains(&f) && f % 2 == 0,
                left: false,
            },
            (8..35).contains(&f) || (40..63).contains(&f),
        )
    });
    assert_eq!(
        digest,
        "088d8c70d66cef772caf8c6f3f902d547636ada3c8bee111a7c1c1db052fdf61"
    );
}

fn replay(frames: u32, controls: impl Fn(u32) -> (Input, bool)) -> String {
    let rom = std::fs::read(std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap()).unwrap();
    let stage = Stage::load(&rom, 0).unwrap();
    let bank = SpriteBank::from_asoe(&rom).unwrap();
    let mut p = Player::sonic_first_stage(&rom).unwrap();
    p.y = 113920;
    p.grounded = true;
    let mut sprite = SpritePlayer::new(0, 0);
    let mut bytes = Vec::new();

    for f in 0..frames {
        let (input, down) = controls(f);
        p.step_controls(input, down, &stage).unwrap();
        let anim = p.animation();
        sprite.animation = anim.animation;
        sprite.variant = anim.variant as u8;
        sprite.speed_q4_4 = anim.speed_q4_4;
        sprite.tick(&bank).unwrap();
        if sprite.ended {
            p.animation_finished();
        }
        let values = [
            p.x,
            p.y,
            p.vx,
            p.vy,
            p.ground_speed,
            p.half_height,
            i32::from(p.angle),
            p.spindash_charge(),
            i32::from(p.animation().animation),
            i32::from(p.animation().variant),
        ];
        for value in values {
            bytes.extend(value.to_le_bytes());
        }
    }

    binary::sha256(&bytes)
}
