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

#[test]
#[ignore = "requires SA2_INSTALL pointing at the original PC installation"]
fn story_landmarks_keep_their_native_ground_clearance() {
    use sa2_asset_tools::city_stage::CityLayout;
    use std::{fs, path::PathBuf};
    let root = PathBuf::from(std::env::var_os("SA2_INSTALL").unwrap());
    let data = root.join("resource/gd_PC");
    let layout = CityLayout::load(
        &fs::read(data.join("set0013_s.bin")).unwrap(),
        &fs::read(root.join("sonic2app.exe")).unwrap(),
    )
    .unwrap();
    for stage in [
        city_land::read_collision_stage(&fs::read(data.join("stg13D.rel")).unwrap()).unwrap(),
        city_land::read_win32_collision_stage(
            &fs::read(data.join("DLL/Win32/Data_DLL.dll")).unwrap(),
        )
        .unwrap(),
    ] {
        let world = collision::bake_stage(&stage, 1.).unwrap();
        let mut observed = Vec::new();
        for (slot, placement) in layout.placements.iter().enumerate() {
            let name = &layout.object_table[usize::from(placement.object_id)].name;
            if !matches!(name.as_str(), "GOALRING" | "SAVEPOINT" | "BOARD") {
                continue;
            }
            let mut origin = placement.position.map(f64::from);
            origin[1] += 1.;
            let gap = world
                .raycast(origin, [0., -1., 0.], 256.)
                .unwrap()
                .map(|h| h.distance - 1.);
            observed.push((slot, name.as_str(), gap));
        }
        // Static geometry only: no mission selection, trigger volume, or moving objects.
        let expected = [
            (21, "GOALRING", Some(36.)),
            (45, "SAVEPOINT", Some(0.001)),
            (103, "BOARD", None),
            (218, "SAVEPOINT", Some(0.)),
            (343, "SAVEPOINT", Some(0.)),
            (457, "GOALRING", Some(0.)),
        ];
        assert_eq!(observed.len(), expected.len());
        for ((slot, name, gap), (i, n, want)) in observed.into_iter().zip(expected) {
            assert_eq!((slot, name), (i, n));
            match (gap, want) {
                (Some(a), Some(b)) => assert!((a - b).abs() < 0.0001, "slot {slot}: {a}"),
                (None, None) => {}
                _ => panic!("unexpected floor hit at slot {slot}"),
            }
        }
    }
}
