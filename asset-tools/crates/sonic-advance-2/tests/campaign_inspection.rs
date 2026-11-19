use sonic_advance_2_assets::native_campaign_save::{CampaignProfile, SECTOR_BYTES, pack_sector};
use std::{fs, process::Command};

#[test]
fn campaign_cli_recovers_only_in_memory() {
    let mut flash = vec![0xff; 16 * SECTOR_BYTES];
    let profile = CampaignProfile::new_usa();
    let valid = pack_sector(&profile, 5, 123);
    flash[..valid.len()].copy_from_slice(&valid);
    let path = std::env::temp_dir().join(format!("a2ne-inspect-{}.sav", std::process::id()));
    // create_new prevents overwriting any pre-existing save or stale test file.
    use std::io::Write;
    fs::File::create_new(&path)
        .unwrap()
        .write_all(&flash)
        .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_a2ne-assets"))
        .arg("campaign-save")
        .arg(&path)
        .output()
        .unwrap();
    let after = fs::read(&path).unwrap();
    fs::remove_file(path).unwrap();
    assert!(output.status.success(), "{:?}", output.stderr);
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["inMemoryRecovery"]["loadedSector"], 0);
    assert_eq!(report["profile"], serde_json::to_value(profile).unwrap());
    assert_eq!(flash, after);
}
