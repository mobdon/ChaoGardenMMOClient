use sa2_asset_tools::{city_land, collision, stage};
#[test]
#[ignore = "requires private installed City Escape REL and independently split SA Tools level"]
fn installed_land_matches_independent_conversion() {
    let file = |key| std::fs::read(std::env::var(key).unwrap()).unwrap();
    let native = city_land::read_collision_stage(&file("SA2_CITY_LAND_REL")).unwrap();
    let converted = stage::read_stage(&file("SA2_CITY_LAND_ORACLE")).unwrap();
    compare(native, converted);
}
#[test]
#[ignore = "requires private installed Data_DLL.dll and separately split DLL level"]
fn win32_land_matches_independent_conversion() {
    let file = |key| std::fs::read(std::env::var(key).unwrap()).unwrap();
    compare(
        city_land::read_win32_collision_stage(&file("SA2_CITY_LAND_DLL")).unwrap(),
        stage::read_stage(&file("SA2_CITY_LAND_DLL_ORACLE")).unwrap(),
    );
}
fn compare(native: stage::Stage, converted: stage::Stage) {
    assert_eq!(native.entries.len(), converted.entries.len());
    let a = collision::bake_stage(&native, 1.).unwrap();
    let b = collision::bake_stage(&converted, 1.).unwrap();
    for (x, y) in a.entries.iter().zip(&b.entries) {
        assert_eq!(
            (x.geometry_kind, x.surface_flags),
            (y.geometry_kind, y.surface_flags)
        );
    }
    assert_eq!(a.meshes.len(), b.meshes.len());
    for (x, y) in a.meshes.iter().zip(&b.meshes) {
        assert_eq!(x.entry_index, y.entry_index);
        assert_eq!(x.vertices, y.vertices, "entry {} vertices", x.entry_index);
        assert_eq!(
            x.triangles, y.triangles,
            "entry {} triangles",
            x.entry_index
        );
    }
    let hit = a
        .raycast([0., 1000., 0.], [0., -1., 0.], 10000.)
        .unwrap()
        .unwrap();
    assert_eq!(hit.entry_index, 2715);
    assert_eq!(hit.position, [0.; 3]);
    println!(
        "entries {} meshes {} vertices {} triangles {}",
        a.entries.len(),
        a.meshes.len(),
        a.meshes.iter().map(|m| m.vertices.len()).sum::<usize>(),
        a.meshes.iter().map(|m| m.triangles.len()).sum::<usize>()
    );
}
