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

#[test]
#[ignore = "run tests/emerald_path_ppc.py with installed stage"]
fn placed_sequences() {
    use sa2_asset_tools::emerald_path::{unrotated_path_translation, PathTaskWork};
    type Frame = (usize, f32, [f32; 3], i32, bool, [f32; 3]);
    type Placement = (
        Vec<[f32; 3]>,
        Vec<f32>,
        Vec<f64>,
        [f32; 3],
        [f32; 3],
        f32,
        Vec<Frame>,
    );
    let data = std::fs::read(std::env::var_os("SA2_PLACED_PATHS").unwrap()).unwrap();
    let placements: Vec<Placement> = serde_json::from_slice(&data).unwrap();
    assert_eq!(placements.len(), 8);
    for (points, lengths, angles, origin, center, distance, frames) in placements {
        let translation = unrotated_path_translation(origin, [0, 0], center).unwrap();
        let mut work = PathTaskWork::default();
        assert!(work.begin(0, |_, _| std::num::NonZeroU32::new(1), Some(|| 255)));
        assert_eq!(frames.len(), 64);
        for expected in frames {
            let (s, w) = work
                .step_placed(
                    &points,
                    translation,
                    distance,
                    |i| lengths[i],
                    |i, _, _| angles[i],
                )
                .unwrap();
            assert_eq!(
                (s.segment, s.fraction, s.position, s.heading, s.wrapped, w),
                expected
            );
        }
    }
}
