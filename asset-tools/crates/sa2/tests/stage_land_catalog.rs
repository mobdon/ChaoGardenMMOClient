use sa2_asset_tools::{collision, stage_land};
#[test]
#[ignore = "requires the installed PC stage modules"]
fn read_installed_land_catalog() {
    let root = std::path::PathBuf::from(std::env::var_os("SA2_GD_PC").unwrap());
    let mut fail = Vec::new();
    for row in stage_land::catalog() {
        if !row.supported {
            continue;
        }
        let file = std::fs::read(root.join(row.filename)).unwrap();
        if let Err(e) =
            stage_land::read_named(&file, row.symbol).and_then(|s| collision::bake_stage(&s, 1.0))
        {
            fail.push(format!("{} {} {}", row.filename, row.symbol, e));
        }
    }
    assert!(fail.is_empty(), "{}", fail.join("\n"));
}

/// Each named input must resolve unambiguously; a content hash can have multiple
/// tables but cannot identify another file or a second offset for one symbol.
#[test]
fn catalog_identities_are_unambiguous() {
    let mut files = std::collections::BTreeMap::new();
    let mut symbols = std::collections::BTreeSet::new();
    for row in stage_land::catalog() {
        assert!(symbols.insert((row.sha256, row.symbol)));
        if let Some(previous) = files.insert(row.sha256, (row.filename, row.size, row.module)) {
            assert_eq!(previous, (row.filename, row.size, row.module));
        }
    }
    assert!(stage_land::read(&[]).is_err());
}
