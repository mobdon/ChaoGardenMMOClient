use sa2_asset_tools::story_movie;
#[test]
#[ignore = "requires installed SA2 story movies"]
fn compare_all_movie_streams() {
    let root = std::path::PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap())
        .join("resource/gd_PC/event");
    let cases: &[(i16, &[&str])] = &[
        (
            401,
            &[
                "232d1fd3bf9d90f56c52cb1556058d856ceaa8827d243a51e9658e7f73a0c1bf",
                "6479eb2f700009f4d58a1cbb1ec8286bad51bac2139476d1602bfbe260700bb6",
            ],
        ),
        (
            409,
            &[
                "5f73ba1073e569c6a2560acbfc50a98cad3c9cbcc918dd7e8c2b3250027af6da",
                "972ba46eb1c6e2ea43e44436561c8033b83080cb09529dc56834b8ad9adf687a",
                "972ba46eb1c6e2ea43e44436561c8033b83080cb09529dc56834b8ad9adf687a",
            ],
        ),
        (
            411,
            &[
                "18642b7910266ae5db412d3b3af69d2c6b75aafd4b5590d5475c67ccbf7b7df5",
                "b65d7b66b8d571e41337677dc4e2cd1c34f64339c8ce14e67531d87969922e09",
                "4f2d1562396ff0003fe5b94ba896fc8606ca38560649193deb71a3229fc573c6",
            ],
        ),
        (
            420,
            &[
                "aa07bafa36a9a0b4ac31e805974348071db2b5711987006e6c74531dce8d8bea",
                "a587f6e1d6591363b74cd0640275b25097eebe843cff5cb776c0d436a95c64cf",
                "09b885038c243227228a430ff6b5e2e9d7266323db3e5205f56f69e20d055f8b",
            ],
        ),
        (
            429,
            &[
                "ab12d8bcdbccbb8a94f4822e2e14aec0b053ece230052de762ac6138e9d5ec9b",
                "9cfe4626cf77a188f84ce90cfcb54065c9afa977cb4c03cf24cbe2d2b3f7295d",
                "18dbfef46e0cd32efa8bf73a5172aeb3544166fe263206f4a20a4763c507694a",
            ],
        ),
        (
            430,
            &[
                "ece5710938c9ff2edd6ae5fa08d0e7bb6c15502b066e1995d727a67059dbc339",
                "d6d27c8550afdbd31947bfac19fdb589d587ae08095885fb542d688c59a38382",
                "7a22c2a6e2d2ab1e6a3d493333b4aca4602a401f849cf8f87188b049ddb2690b",
            ],
        ),
        (
            524,
            &[
                "3194db807346ed8a97c53301a316dbb2953c6f0765c5f4d8c9a300e25fa2f67a",
                "45b77ef5824c8e27ef869824e75b7da599ff37d79572784c78fb4d4a6d4c2f3e",
                "767da0c4582025385af558edf379d0e1c9dd52dee9211f4257643ebd54129472",
            ],
        ),
        (
            532,
            &[
                "4a4eb86104cf657acf39214da966b6843108d393c2366f1cfcc290ad19435704",
                "ad1c1fd0797131cbb934b590c31acd7fbc3cec3c35b14af0312eb2721243694a",
                "bded1c054914ce135c9d61833698aabcd78d342671bca2697b8022d71681915a",
            ],
        ),
        (
            602,
            &[
                "6e3edbdfdd019dbab1ce70be78b751c076891b704895bbc7dbb3d9c51d577640",
                "cb102a3396f2e3eba63146e93f3373d5ca082f38cba8f9c8240e0ad5d8c9cbae",
                "66cdcb63ed7463912e5e646804444c030916054ce30ed556343af7966e2b31b2",
            ],
        ),
    ];
    for &(id, hashes) in cases {
        let b = std::fs::read(root.join(format!("e{id:04}_m.sfd"))).unwrap();
        let report = story_movie::inspect_movie(id, &b).unwrap_or_else(|e| panic!("{id}: {e}"));
        assert_eq!(report["videoStream"]["sha256"], hashes[0]);
        for (key, h) in ["c0", "c1"].iter().zip(&hashes[1..]) {
            assert_eq!(report["audioTracks"][*key]["adxSha256"], *h);
        }
        assert_eq!(
            report["audioTracks"].as_object().unwrap().len(),
            hashes.len() - 1
        );
        let (pictures, ordered) = match id {
            401 => (335, false),
            409 => (327, false),
            411 => (2371, true),
            420 => (2389, false),
            429 => (721, true),
            430 => (619, true),
            524 => (1010, false),
            532 => (1001, true),
            602 => (891, true),
            _ => unreachable!(),
        };
        assert_eq!(report["videoStream"]["pictures"], pictures);
        assert_eq!(report["presentationOrder"].is_array(), ordered);
    }
}
