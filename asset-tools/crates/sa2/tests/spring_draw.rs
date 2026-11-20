use sa2_asset_tools::{
    spring::{SpringKind, StageSpring},
    spring_draw::{display, Draw, Format},
};

#[test]
fn native_head_inherits_coil_scale_only_on_type_a() {
    use Draw::*;
    for (kind, format, head, coil) in [
        (SpringKind::A, Format::Gc, GcNodeModel(2), GcNodeModel(3)),
        (SpringKind::A, Format::Chunk, ChunkModel(2), ChunkModel(1)),
        (SpringKind::B, Format::Gc, GcNodeModel(1), GcNodeModel(4)),
        (SpringKind::B, Format::Chunk, ChunkModel(5), ChunkModel(4)),
    ] {
        let mut s = StageSpring::new(kind, [0.; 3], [0; 3], [0.; 3]);
        s.wait_timer = 2;
        s.compression = -0.25;
        // Track Y scale and translation with zero node offsets.
        let mut stack = vec![(1., 0.)];
        let mut seen = Vec::new();
        display(&s, format, |cmd| {
            let (sy, y) = *stack.last().unwrap();
            match cmd {
                Push => stack.push((sy, y)),
                Pop(n) => stack.truncate(stack.len() - usize::from(n)),
                Scale(v) => stack.last_mut().unwrap().0 *= v[1],
                Translate(v) => stack.last_mut().unwrap().1 += sy * v[1],
                op if op == head || op == coil => seen.push((op, sy, y)),
                _ => {}
            }
        });
        assert_eq!(stack, [(1., 0.)]);
        if kind == SpringKind::A {
            assert_eq!(seen, [(coil, 0.375, 0.), (head, 0.375, -0.375)]);
        } else {
            assert_eq!(seen, [(head, 0.5, -0.5), (coil, 0.375, 0.)]);
        }
    }
}
