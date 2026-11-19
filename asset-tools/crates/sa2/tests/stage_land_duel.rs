use sa2_asset_tools::duel_platform;
use std::fs;
#[test]
#[ignore = "requires original executable and independently split platform SA1MDL"]
fn platform_matches_standalone_split() {
    let exe = fs::read(
        std::path::PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap()).join("sonic2app.exe"),
    )
    .unwrap();
    let reference = fs::read(std::env::var_os("SA2_DUEL_PLATFORM_ORACLE").unwrap()).unwrap();
    let a = duel_platform::read(&exe).unwrap();
    let b = duel_platform::read_split(&reference).unwrap();
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
    assert_eq!(a.vertices.len(), 24);
    assert_eq!(a.vertices, b.vertices);
    assert_eq!(a.normals, b.normals);
    assert_eq!(
        (a.bounds_center, a.bounds_radius),
        (b.bounds_center, b.bounds_radius)
    );
    assert_eq!(a.material_words, b.material_words);
    assert_eq!(a.meshsets.len(), 1);
    assert_eq!(b.meshsets.len(), 1);
    let (a, b) = (&a.meshsets[0], &b.meshsets[0]);
    assert_eq!(
        (a.material_id, a.polygon_type, a.attributes),
        (b.material_id, b.polygon_type, b.attributes)
    );
    assert_eq!(a.polygons.len(), 5);
    assert_eq!(b.polygons.len(), 5);
    for (x, y) in a.polygons.iter().zip(&b.polygons) {
        assert_eq!((x.reversed, &x.indices), (y.reversed, &y.indices));
    }
}
