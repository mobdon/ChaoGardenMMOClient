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
        assert_eq!(report["pictureTimestampsResolved"], ordered);
        let reference = match id {
            401 => "5f2b13da605198f8d0fa5aa5b0a1a4a835c43f335f3f3022a1e852bf38883d74",
            409 => "789878af724ad5e4c058534757145a5fe20f1a9663596ee21416fdac51bd0718",
            411 => "9ab56091e306ef98f1d8d066609d4619d37e356a5f9ef787d55679b1a64e313b",
            420 => "0696eae9cd46e69eaad2b5c093ca530a03b72e342621cc0f2cb61807dde2f203",
            429 => "2e7477afd4c08c6cbebba7e10fd514b3a265c145235e9c05688e5e6ef59f59b3",
            430 => "31365c49cdbf5af7dc6b824d8e532f08e2977cc69b26e11abc0d82e6c471050e",
            524 => "f9040cc6ba262c8d90955ec40940a087c56f307fff8e7d64a549e27fb42780d6",
            532 => "b267e192cc03b93e6b638b677aabe706d7e6677ad88021fd3fb24fdb3d9c02cc",
            602 => "11f03e0d5adace9772b2daa188c38bc902e86c49992850802975443754fe97df",
            _ => unreachable!(),
        };
        let mut rows = Vec::new();
        let mut pts_rows = Vec::new();
        for (i, pic) in report["presentationOrder"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
        {
            if let Some(pts) = pic["pts"].as_u64() {
                pts_rows.extend((i as u32).to_be_bytes());
                pts_rows.extend(pts.to_be_bytes());
                pts_rows.extend(pic["packetOffset"].as_u64().unwrap().to_be_bytes());
            } else {
                assert!(!ordered);
            }

            rows.extend((pic["codedIndex"].as_u64().unwrap() as u32).to_be_bytes());
            rows.extend(pic["coding"].as_str().unwrap().bytes());
        }
        if !ordered {
            let reference = match id {
                401 => "82873319ca9c25db49497a399aec39233fd4290f0ee920e15d0b1008f9fc0e33",
                409 => "956a82b8b1cae48de41b4891b47eda076e13ba11db6d9bdf79092b8b164732cf",
                420 => "5b0163a921b01c3ea7c8f5e60c2df1d5ee91939a0101ab91c628c836096625c8",
                524 => "20e404c431136c8498f6dc2dffc5171b5e25381648a82d9ee2045e0e8301f9f7",
                _ => unreachable!(),
            };
            assert_eq!(sa2_asset_tools::manifest::sha256(&pts_rows), reference);
        }
        assert_eq!(sa2_asset_tools::manifest::sha256(&rows), reference);
    }
}
