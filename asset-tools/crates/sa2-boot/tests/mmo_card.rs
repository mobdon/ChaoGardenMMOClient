//! Round trip against a real ChaoGardenMMOServer (not a mock): start one,
//! make a session, then
//! `SA2_MMO_TEST_SERVER=http://127.0.0.1:PORT SA2_MMO_TEST_TOKEN=... cargo test --test mmo_card -- --ignored`.
//! The card bytes are synthetic test input, never game data.
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

    mmo::pull(|_| panic!("a new account's card is empty")).unwrap();

    // The whole card: the story save beside the Chao garden save.
    let garden: Vec<u8> = (0..65_536u32).map(|i| (i % 253) as u8).collect();
    let card = vec![
        ("SONIC2B__ALF".to_owned(), vec![3; 24], garden.clone()),
        ("SONIC2B__S01".to_owned(), vec![4; 24], vec![9; 24_576]),
    ];
    mmo::push(&card[..1]).unwrap();
    mmo::push(&card).unwrap(); // a second write follows the new revision

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
    assert_eq!(mmo::unpack(&bytes), Some(card));
}
