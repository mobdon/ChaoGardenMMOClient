#[test]
#[ignore = "needs local ASOE"]
fn every_confirmation_frame_matches_retail_for_all_characters() {
    use sonic_advance_assets::{binary, character_select::screen::Screen};
    let rom = binary::read_bounded(
        std::path::Path::new(&std::env::var_os("SONIC_ADVANCE_ASOE_ROM").unwrap()),
        binary::ROM_LEN,
    )
    .unwrap();
    for (idx, json) in [
        include_str!("retail-confirm-sonic.json"),
        include_str!("retail-confirm-tails.json"),
        include_str!("retail-confirm-knuckles.json"),
        include_str!("retail-confirm-amy.json"),
    ]
    .iter()
    .enumerate()
    {
        let hashes: std::collections::BTreeMap<String, String> =
            serde_json::from_str(json).unwrap();
        assert_eq!(hashes.len(), 57);
        let mut s = Screen::load(&rom, 0).unwrap();
        let mut out = vec![0; 240 * 160 * 4];
        for f in 1401..=1657 {
            if f >= 1601 {
                s.render_into(&mut out).unwrap();
                let raw: Vec<_> = out
                    .chunks_exact(4)
                    .flat_map(|p| {
                        (u16::from(p[0] >> 3)
                            | (u16::from(p[1] >> 3) << 5)
                            | (u16::from(p[2] >> 3) << 10))
                            .to_le_bytes()
                    })
                    .collect();
                assert_eq!(
                    binary::sha256(&raw),
                    hashes[&f.to_string()],
                    "character {idx} frame {f}"
                );
            }
            let held = if [1480, 1520, 1560][..idx].contains(&f) {
                16
            } else if f == 1600 {
                1
            } else {
                0
            };
            s.step(held).unwrap();
        }
        assert!(s.confirmation_finished());
    }
}
