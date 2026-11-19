//! Actual-ROM map identities and independently rendered output fingerprints.
use serde::Deserialize;
use sonic_advance_2_assets::{VerifiedRom, catalog, gba, graphics};

#[derive(Deserialize)]
struct Oracle {
    renders: Vec<RenderOracle>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenderOracle {
    group_id: String,
    width_pixels: usize,
    height_pixels: usize,
    rgba_sha256: String,
    map_git_blob_sha1: String,
}

#[test]
#[ignore = "Set A2NE_ROM to the verified local cartridge; independent_parity.py supplies metadata-only image hashes"]
fn native_map_descriptors_identities_and_rgba_match_independent_source_rendering() {
    let path = std::env::var_os("A2NE_ROM").expect("set A2NE_ROM");
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 16_777_216);
    let bytes = std::fs::read(path).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let oracle: Oracle = serde_json::from_slice(
        &std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/catalog/parity.json")).unwrap(),
    )
    .unwrap();
    let resources = catalog::resources().unwrap();
    let groups = [
        (
            "rom.bg.title",
            32,
            64,
            "a4714d7a583f59066b831b1b6ca385b19dfa9da0",
        ),
        (
            "rom.bg.zone-1-act-1",
            32,
            30,
            "ed152fad765ae78e379ff848e352f2d271c94333",
        ),
        (
            "rom.ui.enter-name",
            30,
            20,
            "009d71b34de550db89dfbfca14b7c41bd58f77aa",
        ),
    ];
    assert_eq!(oracle.renders.len(), groups.len());
    for (group, width_tiles, height_tiles, expected_git_blob) in groups {
        let find = |suffix: &str| {
            resources
                .iter()
                .find(|r| r.id == format!("{group}.{suffix}"))
                .unwrap()
        };
        let raw = |suffix: &str| {
            let resource = find(suffix);
            rom.range(&resource.source, resource.offset, resource.length)
                .unwrap()
        };
        let descriptor = raw("descriptor");
        assert_eq!(descriptor.len(), 28);
        let word = |i| u16::from_le_bytes(descriptor[i..i + 2].try_into().unwrap()) as usize;
        let long = |i| u32::from_le_bytes(descriptor[i..i + 4].try_into().unwrap()) as usize;
        assert_eq!((word(0), word(2)), (width_tiles, height_tiles));
        assert_eq!(
            &descriptor[4..8],
            &[0; 4],
            "animated tiles require separate interpretation"
        );
        assert_eq!(long(8), 0x08000000 + find("tiles").offset);
        assert_eq!(long(12), find("tiles").length);
        assert_eq!(long(16), 0x08000000 + find("palette").offset);
        assert_eq!(word(20), 0);
        assert_eq!(word(22) * 2, find("palette").length);
        assert_eq!(long(24), 0x08000000 + find("map").offset);
        let map = raw("map");
        let mut blob = format!("blob {}\0", map.len()).into_bytes();
        blob.extend_from_slice(map);
        assert_eq!(
            gba::sha1(&blob),
            expected_git_blob,
            "{group} source identity"
        );
        let tiles = graphics::decode_4bpp_tiles(raw("tiles"), 16384).unwrap();
        let palette = graphics::parse_palette(raw("palette"), 256).unwrap();
        let entries = graphics::parse_tilemap(map, width_tiles, height_tiles, 65536).unwrap();
        let rendered = graphics::render_text_map(
            &tiles,
            &entries,
            &palette,
            graphics::RenderOptions {
                width_tiles,
                height_tiles,
                depth: graphics::TileDepth::FourBpp,
                transparent_zero: true,
                max_pixels: graphics::MAX_PIXELS,
            },
        )
        .unwrap();
        let expected = oracle.renders.iter().find(|r| r.group_id == group).unwrap();
        assert_eq!(expected.map_git_blob_sha1, expected_git_blob);
        assert_eq!(
            (rendered.width, rendered.height),
            (expected.width_pixels, expected.height_pixels)
        );
        assert_eq!(
            gba::sha256(&rendered.rgba),
            expected.rgba_sha256,
            "{group} RGBA output"
        );
    }
}
