use sonic_advance_assets::{binary, character_select::screen::Screen};
#[test]
#[ignore = "requires local ASOE ROM"]
fn carousel_matches_complete_retail_frames() {
    let path = std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap();
    let rom = binary::read_bounded(std::path::Path::new(&path), binary::ROM_LEN).unwrap();
    let rows: Vec<[u32; 9]> =
        serde_json::from_str(include_str!("retail-character-input.json")).unwrap();
    let hashes: std::collections::BTreeMap<String, String> =
        serde_json::from_str(include_str!("retail-character-video.json")).unwrap();
    let mut scene = Screen::load(&rom, 0).unwrap();
    let mut out = vec![0; 240 * 160 * 4];
    assert_eq!(hashes.len(), 200);
    for frame in 1401..=1600 {
        let want = &hashes[&frame.to_string()];
        scene.render_into(&mut out).unwrap();
        let packed: Vec<u8> = out
            .chunks_exact(4)
            .flat_map(|p| {
                (u16::from(p[0] >> 3) | (u16::from(p[1] >> 3) << 5) | (u16::from(p[2] >> 3) << 10))
                    .to_le_bytes()
            })
            .collect();
        assert_eq!(&binary::sha256(&packed), want, "retail frame {frame}");
        let buttons = if frame < 1415 {
            0
        } else {
            rows[(frame - 1415) as usize][1] as u16
        };
        scene.step(buttons).unwrap();
    }
    let mut short = [9; 4];
    assert!(scene.render_into(&mut short).is_err());
    assert_eq!(short, [9; 4]);
}
