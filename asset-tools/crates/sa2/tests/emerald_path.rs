use sa2_asset_tools::emerald_path::advance;

#[test]
#[ignore = "run tests/emerald_path_ppc.py with the installed REL"]
fn compare_installed_instructions() {
    type Case = (
        Vec<[f32; 3]>,
        Vec<f32>,
        Vec<i16>,
        usize,
        f32,
        (usize, f32, [f32; 3], i32, bool),
    );
    let bytes = std::fs::read(std::env::var_os("SA2_PATH_ORACLE").unwrap()).unwrap();
    let cases: Vec<Case> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(cases.len(), 168);
    for (points, lengths, headings, segment, distance, expected) in cases {
        let actual = advance(
            &points,
            segment,
            0.5,
            distance,
            |i| lengths[i],
            |i| headings[i],
        )
        .unwrap();
        assert_eq!(
            (
                actual.segment,
                actual.fraction,
                actual.position,
                actual.heading,
                actual.wrapped
            ),
            expected
        );
    }
}
