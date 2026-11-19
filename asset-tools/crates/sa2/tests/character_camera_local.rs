use sa2_asset_tools::character_motions::Bank;
use sha2::{Digest, Sha256};
#[test]
#[ignore = "compares installed character camera clips to SAModel split file evaluations"]
fn super_character_camera_tracks() {
    let gd = std::path::PathBuf::from(std::env::var_os("SA2_SET_DIR").unwrap());
    for (stem, slot, frames, digest) in [
        (
            "ssonic",
            106,
            90,
            "5dbf75da666786026aaf523cbf492a207db1154e863793dbf993a840b3eba379",
        ),
        (
            "ssonic",
            108,
            64,
            "8be36c91e246ef96398cc98095feef827e8fe9129a411474424c412f0bf29f47",
        ),
        (
            "sshadow",
            111,
            90,
            "2c415b0aef3d55be4539883b9952be17fb251125ab98df3fd40660c591af0ebb",
        ),
        (
            "sshadow",
            115,
            64,
            "7eeb3a4fa097eb6f8fd0554572e4b34f49ab432b4403058b1d892747bd9583a5",
        ),
    ] {
        let bank = Bank::read(&std::fs::read(gd.join(format!("{stem}mtn.prs"))).unwrap()).unwrap();
        let mut hash = Sha256::new();
        for half in 0..frames * 2 {
            let c = bank.camera(slot, half as f32 * 0.5).unwrap();
            for x in c.position.into_iter().chain(c.target) {
                hash.update(x.to_le_bytes());
            }
            hash.update(c.roll_bams.to_le_bytes());
            hash.update(c.fov_bams.to_le_bytes());
        }
        assert_eq!(
            format!("{:x}", hash.finalize()),
            digest,
            "{stem} slot {slot}"
        );
        assert!(bank.camera(slot, frames as f32).is_err());
        assert!(bank.camera(slot, f32::NAN).is_err());
        assert!(bank.camera(87, 0.).is_err());
    }
}
