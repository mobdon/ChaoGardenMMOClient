use sa2_asset_tools::physical_board::{BoardRider, PhysicalBoard};
use std::fmt::Write;

fn record(out: &mut String, board: &PhysicalBoard, detached: bool) {
    let pos = board.position.map(f32::to_bits);
    let speed = board.speed.map(f32::to_bits);
    writeln!(
        out,
        "{} {} {} {} {} {} {} {} {} {} {} {} {} {}",
        board.slot,
        board.mode,
        u8::from(detached),
        pos[0],
        pos[1],
        pos[2],
        board.angles[0],
        board.angles[1],
        board.angles[2],
        speed[0],
        speed[1],
        speed[2],
        board.cached_action,
        board.cached_frame.to_bits()
    )
    .unwrap();
}

fn rust_trace() -> String {
    let mut output = String::new();
    let actions = [0, 0x78, 0x79, 0x90, 0x91, -1, 0x79, 0x80, 0x90, 0, 0x80];
    let modes = [0, 0, 3, 2, 3, 5, 3, -1, 4, 0, 2];
    for x in [0, 1, 0xfffe, 0xffff] {
        for allocated in [false, true] {
            let Some(mut board) =
                PhysicalBoard::bind([x, 0x8000, 0xffff], [-1.0, 2.5, -3.0], allocated)
            else {
                output.push_str("unbound\n");
                continue;
            };
            record(&mut output, &board, false);
            for frame in 0..actions.len() {
                let i = frame as i32 + i32::from(x & 1) * 20;
                let rider = if frame == 0 || frame == 5 {
                    None
                } else {
                    Some(BoardRider {
                        position: [i as f32 + 0.5, -(i as f32), 5.0],
                        angles: [-i, 40000 - i, i * 1234],
                        speed: [i as f32 * 0.25, -2.0, 3.0],
                        action: actions[frame],
                        motion_mode: modes[frame],
                        frame: i as f32 + 0.75,
                    })
                };
                let detached = board.tick(rider);
                record(&mut output, &board, detached);
            }
        }
    }
    output
}

#[test]
#[ignore = "requires SA2B_REFERENCE pinned to 874a480, a host C compiler and Python"]
fn compares_pinned_physical_board_c() {
    let python = std::env::var_os("PYTHON").unwrap_or_else(|| "python".into());
    let harness =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/physical_board_reference.py");
    let result = std::process::Command::new(python)
        .arg(harness)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let reference = String::from_utf8(result.stdout).unwrap();
    let actual = rust_trace();
    assert_eq!(reference.lines().count(), 52);
    assert_eq!(actual.lines().count(), 52);
    for (line, (c, rust)) in reference.lines().zip(actual.lines()).enumerate() {
        assert_eq!(rust, c, "board callback trace row {line}");
    }
}

#[test]
#[ignore = "optional; requires SA2_SET_DIR and Python with unicorn 2.1.4"]
fn compares_installed_ppc_object_board_exec() {
    let python = std::env::var_os("PYTHON").unwrap_or_else(|| "python".into());
    let harness =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/physical_board_ppc.py");
    let output = std::process::Command::new(python)
        .arg(harness)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reference = String::from_utf8(output.stdout).unwrap();
    let trace = rust_trace();
    // Each rotation starts with one failed bind and one successful bind.
    let exec: Vec<_> = trace
        .lines()
        .enumerate()
        .filter_map(|(i, s)| (i % 13 >= 2).then_some(s))
        .collect();
    assert_eq!(exec.len(), 44);
    let actual: Vec<_> = reference.lines().collect();
    assert_eq!(actual.len(), exec.len());
    for (i, (ppc, rust)) in actual.into_iter().zip(exec).enumerate() {
        assert_eq!(rust, ppc, "PPC board executor frame {i}");
    }
}
