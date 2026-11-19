use serde::Deserialize;
use sonic_advance_assets::{
    binary,
    title::{
        menu::{Destination, MainMenu},
        Title, TitleEvent,
    },
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Sample {
    tick: u32,
    retail_frame: u32,
    bgr555_sha256: String,
}
fn hash(pixels: &[u8]) -> String {
    let packed: Vec<_> = pixels
        .chunks_exact(4)
        .flat_map(|p| {
            (u16::from(p[0] >> 3) | (u16::from(p[1] >> 3) << 5) | (u16::from(p[2] >> 3) << 10))
                .to_le_bytes()
        })
        .collect();
    binary::sha256(&packed)
}
#[test]
#[ignore = "requires local ASOE; menu pixel oracles from retail video"]
fn menu_matches_cold_boot_video_on_both_routes() {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap();
    let rom = binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN).unwrap();
    for (tiny, fixture) in [
        (false, include_str!("retail-menu.json")),
        (true, include_str!("retail-menu-tiny.json")),
    ] {
        let expected: Vec<Sample> = serde_json::from_str(fixture).unwrap();
        let mut title = Title::load_asoe(&rom).unwrap();
        for _ in 0..197 {
            title.step(0, 1).unwrap();
        }
        assert_eq!(hash(&title.render().unwrap()), expected[0].bgr555_sha256);
        assert_eq!(title.step(8, 1).unwrap(), TitleEvent::MainMenuRequested);
        let mut menu = MainMenu::from_title(&rom, &title, 8).unwrap();
        let mut out = title.render().unwrap();
        for frame in 0..=120u32 {
            if frame > 0 {
                let held = match frame {
                    10 => 64,
                    20..=64 if !tiny => 128,
                    70 => 1,
                    _ => 0,
                };
                menu.step(held, 1).unwrap();
            }
            if let Some(sample) = expected.iter().find(|s| s.tick == frame + 1) {
                menu.render_into(&mut out).unwrap();
                assert_eq!(
                    hash(&out),
                    sample.bgr555_sha256,
                    "retail frame {} tiny={tiny}",
                    sample.retail_frame
                );
            }
        }
        assert_eq!(
            menu.snapshot().destination,
            Some(if tiny {
                Destination::TinyGarden
            } else {
                Destination::Options
            })
        );
        let before = menu.snapshot().frame;
        assert!(menu.step(0x400, 2).is_err());
        assert!(menu.step(0, 5).is_err());
        assert_eq!(before, menu.snapshot().frame);
        let mut short = [177; 16];
        assert!(menu.render_into(&mut short).is_err());
        assert_eq!(short, [177; 16]);
    }
}
