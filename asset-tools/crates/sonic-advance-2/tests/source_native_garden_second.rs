use sonic_advance_2_assets::{
    VerifiedRom, gba,
    native_garden_second::{self as bounce, BouncePhase, BouncePractice},
};

#[test]
fn bounce_identity_and_native_entrypoints_are_explicit() {
    assert!(bounce::A2neBounceSource::from_verified_payload(&[]).is_err());
    assert!(bounce::A2neBounceSource::from_verified_payload(&vec![0; 191_340]).is_err());
    assert_eq!(bounce::INITIALIZE_ADDRESS, 0x0200_d294);
    assert_eq!(bounce::CHAO_ADDRESS, 0x0200_d958);
    assert_eq!(bounce::SPRING_ADDRESS, 0x0200_e964);
    assert_eq!(bounce::TARGETS, 26);
}

#[test]
#[ignore = "requires verified A2NE_ROM"]
fn cartridge_inputs_run_a_complete_bounce_round_without_save_or_service() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").unwrap()).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let mut practice = BouncePractice::new(&rom).unwrap();
    let before = practice.view();
    assert_eq!(practice.tick(0x100), Err(bounce::Error::InvalidInput));
    assert_eq!(practice.view(), before);
    assert_eq!(practice.frames(), 0);
    let mut transcript = Vec::new();
    for frame in 0..20_000 {
        let held = u16::from(frame % 60 == 0);
        let view = practice.tick(held).unwrap();
        transcript.extend(serde_json::to_vec(&view).unwrap());
        if practice.finished().is_some() {
            break;
        }
    }
    let result = practice
        .finished()
        .expect("native life/result flow must complete");
    assert_eq!(practice.frames(), 1097);
    assert_eq!(result, (false, 7));
    assert_eq!(
        gba::sha256(&transcript),
        "f80c7e0f8fe557798ed369dec9c651927db2dc1d283df3b30343f0df6c29d639"
    );
    assert_eq!(practice.view().phase, BouncePhase::Ended);
    let frames = practice.frames();
    let view = practice.view();
    assert_eq!(practice.tick(0).unwrap(), view);
    assert_eq!(practice.frames(), frames);
    println!(
        "BOUNCE_PRACTICE {}",
        serde_json::json!({"frames":frames,"won":result.0,"rings":result.1,"viewTranscriptSha256":gba::sha256(&transcript),"saveWrites":0,"serviceCalls":0})
    );
}
