use sa2_asset_tools::emerald_path::{advance, segment_heading};

#[test]
#[ignore = "run tests/emerald_path_ppc.py with the installed REL"]
fn compare_installed_instructions() {
    type Case = (
        Vec<[f32; 3]>,
        Vec<f32>,
        Vec<f64>,
        Vec<(usize, [u64; 2])>,
        usize,
        f32,
        (usize, f32, [f32; 3], i32, bool),
    );
    let bytes = std::fs::read(std::env::var_os("SA2_PATH_ORACLE").unwrap()).unwrap();
    let cases: Vec<Case> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(cases.len(), 168);
    for (points, lengths, angles, calls, segment, distance, expected) in cases {
        let observed = std::cell::RefCell::new(Vec::new());
        let actual = advance(
            &points,
            segment,
            0.5,
            distance,
            |i| lengths[i],
            |i| {
                segment_heading(points[i], points[(i + 1) % points.len()], |x, z| {
                    observed
                        .borrow_mut()
                        .push((i, [f64::from(x).to_bits(), f64::from(z).to_bits()]));
                    angles[i]
                })
            },
        )
        .unwrap();
        assert_eq!(observed.into_inner(), calls);
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
