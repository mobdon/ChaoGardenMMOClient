//! Round trip against a real ChaoGardenMMOServer (not a mock): start one,
//! make a session, then
//! `SA2_MMO_TEST_SERVER=http://127.0.0.1:PORT SA2_MMO_TEST_TOKEN=... cargo test --test mmo_card -- --ignored`.
//! The save bytes are synthetic test input, never game data.
#![cfg(not(target_os = "wasi"))]
use sa2_boot::mmo;
use std::io::Read;

#[test]
#[ignore = "needs a running server"]
fn server_card_checks_out_saves_and_holds_the_lease() {
    let server = std::env::var("SA2_MMO_TEST_SERVER").expect("SA2_MMO_TEST_SERVER");
    let token = std::env::var("SA2_MMO_TEST_TOKEN").expect("SA2_MMO_TEST_TOKEN");
    std::env::set_var("SA2_MMO_SERVER", &server);
    std::env::remove_var("SA2_MMO_TOKEN");
    assert!(mmo::configured() && !mmo::signed_in(), "no token: no card");
    std::env::set_var("SA2_MMO_TOKEN", &token);
    assert!(mmo::signed_in());

    let dir = std::env::temp_dir().join(format!("mmo-card-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cache = dir.join("garden.dat");
    mmo::pull(&cache).unwrap();
    assert!(mmo::push(&[]).is_err(), "the server refuses an empty save");

    let save: Vec<u8> = (0..60_000u32).map(|i| (i % 253) as u8).collect();
    mmo::push(&save).unwrap();
    mmo::push(&save[..1000]).unwrap(); // a second write follows the new revision

    // Another game can't open the same Chao while this one holds it.
    let other = ureq::get(&format!("{server}/api/game-save"))
        .set("Authorization", &format!("Bearer {token}"))
        .call();
    assert!(matches!(other, Err(ureq::Error::Status(409, _))));

    // Released, the server hands back exactly the last write, revision 2.
    mmo::release();
    let back = ureq::get(&format!("{server}/api/game-save"))
        .set("Authorization", &format!("Bearer {token}"))
        .call()
        .unwrap();
    assert_eq!(back.header("x-save-revision"), Some("2"));
    let mut bytes = Vec::new();
    back.into_reader().read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, save[..1000]);
}
