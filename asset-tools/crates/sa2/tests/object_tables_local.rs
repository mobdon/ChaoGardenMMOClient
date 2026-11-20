use sa2_asset_tools::{manifest, object_table};
use std::{fs, path::PathBuf};

#[test]
#[ignore = "requires the hash-pinned Steam executable and SA Tools 786 ObjectLists.ini"]
fn all_mapped_object_definitions_match_independent_pe_unpack() {
    let exe = fs::read(std::env::var_os("SA2_PC_EXE").unwrap()).unwrap();
    let cfg = PathBuf::from(std::env::var_os("SA2_PC_CONFIG").unwrap()).join("ObjectLists.ini");
    let tables = object_table::mapped_tables(&exe, &fs::read_to_string(cfg).unwrap()).unwrap();
    // The pinned hash is SA Tools' own unpack, which reads Dry Lagoon 2P through
    // the mapped header 0x11A76B0 (Pyramid Race's list). The crate corrects that
    // header to 0x1164DB0 (see object_table::HEAD_CORRECTIONS), so rebuild that
    // one list as SA Tools maps it for this comparison.
    const DRY_LAGOON_2P: &str = "Dry Lagoon 2P Object List";
    let image = object_table::verify_image(&exe).unwrap();
    let sa_tools_view = object_table::read_table(&exe, &image, 0x11A76B0).unwrap();
    assert_eq!(tables[DRY_LAGOON_2P].head_rva, 0x1164DB0);
    assert_eq!(tables[DRY_LAGOON_2P].entries.len(), 100);
    assert_eq!(sa_tools_view.len(), 89);
    let mut canonical = Vec::new();
    let mut count = 0;
    for (section, table) in &tables {
        let (head, entries) = if section == DRY_LAGOON_2P {
            (0x11A76B0u32, &sa_tools_view)
        } else {
            (table.head_rva, &table.entries)
        };
        for (i, d) in entries.iter().enumerate() {
            canonical.extend_from_slice(section.as_bytes());
            canonical.push(0);
            canonical.extend_from_slice(&head.to_le_bytes());
            canonical.extend_from_slice(&(i as u16).to_le_bytes());
            canonical.extend_from_slice(&[d.load_flags, d.list]);
            canonical.extend_from_slice(&d.flags.to_le_bytes());
            canonical.extend_from_slice(&d.distance.to_bits().to_le_bytes());
            canonical.extend_from_slice(&d.function_va.to_le_bytes());
            canonical.extend_from_slice(d.name.as_bytes());
            canonical.push(0);
            count += 1;
        }
    }
    assert_eq!((tables.len(), count), (69, 5104));
    assert_eq!(
        manifest::sha256(&canonical),
        "6d3dd4757a1ce8ebcfc3f643111092b7b60bb98c0c0d88559a26253f1efcf4cb"
    );
}
