use sa2_asset_tools::stage_positions::{for_stage, read_rows, PositionKind, Variant};
use sha2::{Digest, Sha256};

#[test]
#[ignore = "requires SA2_SET_DIR pointing to the installed resource/gd_PC"]
fn matches_all_pc_character_position_tables_read_by_sa_tools() {
    let path = std::path::PathBuf::from(std::env::var_os("SA2_SET_DIR").unwrap())
        .join("../../sonic2app.exe");
    let exe = std::fs::read(path).unwrap();
    let rows = read_rows(&exe).unwrap();
    assert_eq!(rows.len(), 329);
    let mut digest = Sha256::new();
    for row in &rows {
        digest.update(row.stage.to_le_bytes());
        digest.update([row.poses.len() as u8]);
        if let Some(unknown) = row.unknown {
            digest.update(unknown.to_le_bytes());
        }
        for pose in &row.poses {
            digest.update(pose.yaw.to_le_bytes());
            for v in pose.position {
                digest.update(v.to_le_bytes());
            }
        }
    }
    assert_eq!(
        format!("{:x}", digest.finalize()),
        "7112dfb10e976a0a5b0ccacc737a4406192e8ede76b1cd738741284c3d969ea4"
    );
    let city = for_stage(&exe, 13).unwrap();
    assert_eq!(city.len(), 7);
    let sonic = city
        .iter()
        .find(|r| r.character == "Sonic" && r.kind == PositionKind::Start)
        .unwrap();
    assert_eq!(sonic.poses[0].variant, Variant::Single);
    assert_eq!(sonic.poses[0].position, [0., 300., 340.]);
    assert_eq!(sonic.poses[1].position, [10., 300., 340.]);
    assert_eq!(sonic.poses[2].position, [-10., 300., 340.]);
    assert!(for_stage(&exe, 91).unwrap().is_empty());
}
