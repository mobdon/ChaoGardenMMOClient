use sa2_asset_tools::{duel_platform, kart_objects};
use std::{env, fs, path::PathBuf};

#[test]
#[ignore = "requires installed game and SA Tools split kart collision models"]
fn matches_kart_sets_and_split_models() {
    let g = PathBuf::from(env::var_os("SA2_GAME_PATH").unwrap());
    let exe = fs::read(g.join("sonic2app.exe")).unwrap();
    let refs = PathBuf::from(env::var_os("SA2_KART_OBJECT_SPLITS").unwrap());
    for (file, n) in [
        ("setCartBase.bin", 1),
        ("setCartMini1.bin", 24),
        ("setCartMini2.bin", 25),
        ("setCartMini3.bin", 26),
        ("setCartRouge.bin", 29),
        ("setCartRouge_hd.bin", 31),
        ("setCartTails.bin", 34),
        ("setCartTails_hd.bin", 34),
    ] {
        let set = fs::read(g.join("resource/gd_PC").join(file)).unwrap();
        let got = kart_objects::read(&exe, &set).unwrap();
        assert_eq!(got.set_file, file);
        assert_eq!(got.placements.len(), n);
        assert_eq!(got.definitions.len(), 13);
        assert_eq!(got.definitions[1].name, "JUMP PANEL");
        assert_eq!(got.definitions[3].name, "GATE");
        assert_eq!(got.definitions[4].name, "GATE2");
        for (i, p) in got.placements.iter().enumerate() {
            let raw = &set[32 + i * 32..32 + (i + 1) * 32];
            let packed = u16::from_be_bytes(raw[..2].try_into().unwrap());
            assert_eq!(p.object_id, packed & 0xfff);
            for (j, v) in p.position.iter().chain(&p.parameters).enumerate() {
                assert_eq!(
                    v.to_bits(),
                    u32::from_be_bytes(raw[8 + 4 * j..12 + 4 * j].try_into().unwrap())
                );
            }
        }
        if file != "setCartBase.bin" {
            continue;
        }
        for item in got.collision_models {
            let split = duel_platform::read_split(
                &fs::read(refs.join(format!("{}.sa1mdl", item.name))).unwrap(),
            )
            .unwrap();
            let a = item.model;
            let b = split;
            assert_eq!(
                (
                    a.node.flags,
                    a.node.position,
                    a.node.rotation_bams,
                    a.node.scale
                ),
                (
                    b.node.flags,
                    b.node.position,
                    b.node.rotation_bams,
                    b.node.scale
                )
            );
            let (a, b) = (a.attachment, b.attachment);
            assert_eq!(a.vertices, b.vertices);
            assert_eq!(a.normals, b.normals);
            assert_eq!(
                (a.bounds_center, a.bounds_radius),
                (b.bounds_center, b.bounds_radius)
            );
            assert_eq!(a.material_words, b.material_words);
            assert_eq!(a.meshsets.len(), b.meshsets.len());
            for (x, y) in a.meshsets.iter().zip(&b.meshsets) {
                assert_eq!(
                    (
                        x.material_id,
                        x.polygon_type,
                        x.attributes,
                        &x.colors_argb,
                        &x.uv_signed
                    ),
                    (
                        y.material_id,
                        y.polygon_type,
                        y.attributes,
                        &y.colors_argb,
                        &y.uv_signed
                    )
                );
                assert!(x
                    .polygons
                    .iter()
                    .map(|p| (p.reversed, &p.indices))
                    .eq(y.polygons.iter().map(|p| (p.reversed, &p.indices))));
            }
        }
    }
}
