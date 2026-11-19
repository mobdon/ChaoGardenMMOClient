use sonic_advance_assets::{
    binary,
    hud::{Hud, HudState},
};

const BACKGROUND: u32 = 0x01336699;

fn retail() -> Hud {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap();
    Hud::from_asoe(&std::fs::read(path).unwrap()).unwrap()
}
fn render(hud: &Hud, state: HudState) -> Vec<u32> {
    let mut fb = vec![BACKGROUND; 240 * 160];
    hud.draw(&mut fb, state).unwrap();
    fb
}

#[test]
fn wrong_source_is_rejected_before_loading_ui_records() {
    assert!(Hud::from_asoe(&[]).is_err());
}

#[test]
#[ignore = "requires the exact local SONIC_ADVANCE_ASOE_ROM"]
fn full_hud_matches_registered_mgba_oam_and_vram_at_two_frames() {
    let hud = retail();
    for (rings, course_frames, count, digest) in [
        (
            0,
            26,
            1112,
            "059a3dc5b1545fbbb86a07d112e64a1c19a42ab6b46dbba77409f5ba230a92f0",
        ),
        (
            14,
            625,
            1080,
            "00dd58c64e2ce97127f3346154c51dae27f8aedda830a29188a997dcb078f584",
        ),
    ] {
        let fb = render(
            &hud,
            HudState {
                rings,
                course_frames,
                lives: 3,
                ..Default::default()
            },
        );
        assert_eq!(fb.iter().filter(|&&v| v != BACKGROUND).count(), count);
        let bytes: Vec<_> = fb.iter().flat_map(|v| v.to_le_bytes()).collect();
        assert_eq!(binary::sha256(&bytes), digest);
    }
}

#[test]
#[ignore = "requires the exact local SONIC_ADVANCE_ASOE_ROM"]
fn timer_warning_limits_and_zero_ring_flash_follow_stage_rules() {
    let hud = retail();
    let s = HudState {
        course_frames: 35999,
        stage_frames: 0,
        lives: 3,
        ..Default::default()
    };
    let plain = render(&hud, s);
    assert_ne!(
        plain,
        render(
            &hud,
            HudState {
                zero_ring_phase: 8,
                ..s
            }
        )
    );
    assert_eq!(
        plain,
        render(
            &hud,
            HudState {
                zero_ring_phase: 24,
                ..s
            }
        )
    );
    let warning = HudState {
        stage_frames: 16,
        ..s
    };
    assert_ne!(plain, render(&hud, warning));
    assert_eq!(
        plain,
        render(
            &hud,
            HudState {
                time_limit_disabled: true,
                ..warning
            }
        )
    );
    // The last real frame is 9:59:98. At the cap the readout freezes at 9:59:99.
    let cap = HudState {
        course_frames: 36000,
        ..s
    };
    assert_ne!(plain, render(&hud, cap));
    assert_eq!(
        render(&hud, cap),
        render(
            &hud,
            HudState {
                course_frames: u32::MAX,
                ..s
            }
        )
    );
    let countdown = HudState {
        course_frames: 3599,
        timer_reversed: true,
        stage_frames: 16,
        ..s
    };
    assert_ne!(
        render(&hud, countdown),
        render(
            &hud,
            HudState {
                timer_reversed: false,
                ..countdown
            }
        )
    );
    let limit = HudState {
        rings: 999,
        lives: 10,
        ..s
    };
    assert_eq!(
        render(&hud, limit),
        render(
            &hud,
            HudState {
                rings: 1500,
                lives: 255,
                ..limit
            }
        )
    );
    let mut invalid = [0; 16];
    assert!(hud.draw(&mut invalid, s).is_err());
    assert_eq!(invalid, [0; 16]);
}
