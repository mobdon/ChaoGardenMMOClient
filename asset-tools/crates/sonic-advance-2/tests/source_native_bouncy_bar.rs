use sonic_advance_2_assets::{
    VerifiedRom, gba,
    native_animation::{AnimationPlayer, Hitbox},
    native_bouncy_bar::{self as bar, BarState},
    native_player::{Character, PlayerState},
};

#[test]
fn original_c_idle_collision_and_complete_launch_sequences_match() {
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("../catalog/native-bouncy-bar-parity.json")).unwrap();
    let mut seed = 0xb0ba_2026u32;
    let mut next = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        seed
    };
    let mut output = Vec::new();
    for row in 0..4096 {
        let dx = (next() % 257) as i32 - 128;
        let dy = (next() % 49) as i32 - 24;
        let vy = (next() % 8192) as i16 - 1024;
        let move_state = (next() & 0x100) | if row % 17 == 0 { 0x80 } else { 0 };
        let position = (next() % 200) as i16;
        let speed = (next() % 3) as u8;
        let mut p = PlayerState::new_at(
            Character::Sonic,
            (800 + dx) * 256 + (row & 255),
            (400 + dy) * 256 + (row & 255),
        );
        p.q_speed_air_x = 1200;
        p.q_speed_air_y = vy;
        p.q_speed_ground = 1000;
        p.move_state = move_state;
        p.char_state = 14;
        let mut state = BarState {
            launching: row % 3 == 0,
            landing_speed: speed,
            launch_frame: if row % 3 == 0 { (row % 21) as u8 } else { 0 },
            spring_stiffness: 9,
            landing_position: position,
        };
        let mut variant = if state.launching { 2 } else { 0 };
        let body = Hitbox {
            index: if row % 13 == 0 { -1 } else { 0 },
            rectangle: [-6, -14, 6, 14],
        };
        let hitbox = Hitbox {
            index: if row % 11 == 0 { -1 } else { 0 },
            rectangle: [-112, -8, 112, 8],
        };
        for step in 0..24 {
            let was_launching = state.launching;
            let activated = if was_launching {
                state.launch_tick(&mut p);
                false
            } else {
                let overlap = bar::body_overlap(800, 400, hitbox, &p, body);
                state.idle_tick(&mut p, 800, 400, overlap)
            };
            if activated {
                variant = state.active_variant();
            }
            if was_launching && step == 23 {
                state.animation_finished();
                variant = 0;
            }
            let words = [
                p.q_world_x,
                p.q_world_y,
                i32::from(p.q_speed_air_x),
                i32::from(p.q_speed_air_y),
                i32::from(p.q_speed_ground),
                p.move_state as i32,
                i32::from(p.char_state),
                if activated { 5 } else { 0 },
                i32::from(state.landing_speed),
                i32::from(state.launch_frame),
                i32::from(state.spring_stiffness),
                i32::from(state.landing_position),
                i32::from(state.launching),
                i32::from(variant),
                if activated { 279 } else { 0 },
                1,
                0,
                254,
                hitbox.index,
                body.index,
            ];
            output.extend(words.into_iter().flat_map(i32::to_le_bytes));
        }
    }
    assert_eq!(
        output.len(),
        expected["tasks"]["bytes"].as_u64().unwrap() as usize
    );
    assert_eq!(
        gba::sha256(&output),
        expected["tasks"]["sha256"].as_str().unwrap()
    );
}

#[test]
#[ignore = "requires verified A2NE_ROM"]
fn actual_bar_animations_keep_native_frames_hitboxes_and_end_timing() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    for variant in 0..4 {
        let mut animation = AnimationPlayer::new(&rom, bar::ANIMATION, variant).unwrap();
        assert_eq!(animation.hitboxes()[0].index, -1);
        let mut ended = None;
        let mut frames = Vec::new();
        let mut pixels = Vec::new();
        for frame in 0..120 {
            let step = animation.advance().unwrap();
            if let Some(index) = step.frame_index {
                frames.push(index);
            }
            let rendered = animation.render().unwrap();
            pixels.extend(rendered.rgba);
            if step.ended {
                ended = Some(frame);
                break;
            }
        }
        assert!(animation.hitboxes()[0].index != -1 || variant != 0);
        if variant != 0 {
            assert!(ended.unwrap() >= u32::from(25 - 5 * variant));
        }
        println!(
            "BAR_ANIMATION {}",
            serde_json::json!({"variant":variant,"endedFrame":ended,"frames":frames.len(),"frameIndicesSha256":gba::sha256(&frames.into_iter().flat_map(u16::to_le_bytes).collect::<Vec<_>>()),"rgbaBytes":pixels.len(),"rgbaSha256":gba::sha256(&pixels)})
        );
    }
}
