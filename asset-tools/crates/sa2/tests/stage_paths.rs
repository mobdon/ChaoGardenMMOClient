use sa2_asset_tools::stage_paths;
use sha2::{Digest, Sha256};

#[test]
fn unsupported_images_fail() {
    assert!(stage_paths::read(&[], "[x]\ntype=pathlist\naddress=100").is_err());
}

#[test]
#[ignore = "requires SA2_PC_EXE and SA2_PATH_CONFIG from stage_path_reference.ps1"]
fn matches_sa_tools_all_paths() {
    let file = std::fs::read(std::env::var_os("SA2_PC_EXE").unwrap()).unwrap();
    let ini = std::fs::read_to_string(std::env::var_os("SA2_PATH_CONFIG").unwrap()).unwrap();
    let data = stage_paths::read(&file, &ini).unwrap();
    assert_eq!(data.len(), 22);
    assert_eq!(data["City Escape Paths"].paths.len(), 13);
    assert_eq!(
        data.values().filter(|p| p.explicit_count.is_some()).count(),
        4
    );
    assert_eq!(data.values().map(|t| t.paths.len()).sum::<usize>(), 228);
    assert_eq!(
        data.values()
            .flat_map(|t| &t.paths)
            .map(|p| p.points.len())
            .sum::<usize>(),
        10360
    );
    let mut lists = data.values().collect::<Vec<_>>();
    lists.sort_by_key(|l| l.rva);
    for canonical_zero in [false, true] {
        let mut h = Sha256::new();
        for l in &lists {
            h.update(l.rva.to_le_bytes());
            h.update((l.paths.len() as u32).to_le_bytes());
            for p in &l.paths {
                h.update([p.path_type, p.header_byte_1]);
                h.update((p.points.len() as u16).to_le_bytes());
                h.update(
                    (if canonical_zero && p.total_length == 0.0 {
                        0.0
                    } else {
                        p.total_length
                    })
                    .to_le_bytes(),
                );
                h.update(p.callback_va.to_le_bytes());
                for q in &p.points {
                    h.update(q.angle_x.to_le_bytes());
                    h.update(q.angle_z.to_le_bytes());
                    for f in [q.length].into_iter().chain(q.position) {
                        h.update((if canonical_zero && f == 0.0 { 0.0 } else { f }).to_le_bytes());
                    }
                }
            }
        }
        assert_eq!(
            format!("{:x}", h.finalize()),
            if canonical_zero {
                "83fbc1034753bbf00e35679ab73335a8fc98f21b67903289bca1ca11fea3b96d"
            } else {
                "101004e33922f51ac8df76447a36b3fe5f53149446b9c175afd636a4c815bb39"
            }
        );
    }
}

#[test]
#[ignore = "requires SA2_PC_EXE"]
fn rejects_bad_count_and_out_of_section_addresses() {
    let file = std::fs::read(std::env::var_os("SA2_PC_EXE").unwrap()).unwrap();
    for variant in [
        "address=ffffffff",
        "address=CDB35C\ncount=999999",
        "address=CDB35C\ncount=14",
        "address=CDB35C\ncount=-1",
    ] {
        assert!(stage_paths::read(&file, &format!("[x]\ntype=pathlist\n{variant}")).is_err());
    }
}
