use sa2_asset_tools::emerald_path::PathTaskWork;
use serde_json::{json, Value};
use std::{cell::RefCell, num::NonZeroU32};
#[test]
#[ignore = "run emerald_lifetime_ppc.py with installed Wild Canyon REL"]
fn installed_work_lifetime() {
    type Row = (i32, bool, u32, u32, bool, Value, Value, Value);
    let bytes = std::fs::read(std::env::var_os("SA2_PATH_WORK_ORACLE").unwrap()).unwrap();
    let rows: Vec<Row> = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(rows.len(), 36);
    for (flag, set, allocation, saved, started, before, after, expected_calls) in rows {
        let mut work = PathTaskWork {
            fraction: 0.625,
            segment: 0x1357,
            allocation: NonZeroU32::new(0x100800),
        };
        let calls = RefCell::new(Vec::new());
        let state = |w: &PathTaskWork| {
            json!([
                w.fraction,
                w.segment,
                w.allocation.map_or(0, NonZeroU32::get)
            ])
        };
        assert_eq!(
            work.begin(
                flag,
                |count, size| {
                    calls.borrow_mut().push(json!([0x220e4, count, size]));
                    NonZeroU32::new(allocation)
                },
                set.then_some(|| {
                    calls.borrow_mut().push(json!([0x22108]));
                    saved
                })
            ),
            started
        );
        assert_eq!(state(&work), before);
        work.unload(
            set.then_some(|v| calls.borrow_mut().push(json!([0x222c4, v]))),
            |v| calls.borrow_mut().push(json!([0x222d4, v])),
        );
        assert_eq!(state(&work), after);
        assert_eq!(json!(calls.into_inner()), expected_calls);
    }
}
