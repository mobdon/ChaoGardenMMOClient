use sa2_asset_tools::ring_models::{Manager, Mesh};

#[test]
fn failed_task_allocation_retries_and_preserves_native_list_order() {
    let mut m = Manager::default();
    let first = m.create(|| false);
    m.models(false, |_| 0., |_, _, _| panic!("no display task"));
    let second = m.create(|| true);
    let third = m.create(|| panic!("duplicate manager allocation"));
    for (id, z) in [(first, -130.), (second, -65.), (third, 9.)] {
        m.model_mut(id).unwrap().position[2] = z;
    }
    let mut draws = vec![];
    m.models(false, |p| p[2], |id, _, lod| draws.push((id, lod)));
    assert_eq!(
        draws,
        [
            (third, Mesh::RingL1),
            (second, Mesh::RingL2),
            (first, Mesh::Ring)
        ]
    );
    m.model_mut(third).unwrap().position[2] = 10.;
    m.model_mut(second).unwrap().shadow_y = 42.;
    m.retire(second);
    let mut shadow = vec![];
    m.shadows(false, |id, _| shadow.push(id));
    assert_eq!(shadow, [second]);
    assert!(!m.execute());
    m.shadows(false, |_, _| panic!("retired shadow survived cleanup"));
    draws.clear();
    m.models(false, |p| p[2], |id, _, lod| draws.push((id, lod)));
    assert_eq!(draws, [(first, Mesh::Ring)]);
    m.retire(first);
    m.retire(third);
    assert!(m.execute());
    m.destroy();
    assert!(!m.execute());
    let next = m.create(|| true);
    assert_ne!(next, first);
    assert!(m.model_mut(first).is_none());
}

#[test]
fn separate_passes_restore_fog_and_start_each_draw_from_the_view_matrix() {
    use sa2_asset_tools::ring_models::{Draw::*, Pass};
    let mut m = Manager::default();
    let id = m.create(|| true);
    let n = m.model_mut(id).unwrap();
    n.position = [5., 10., 15.];
    n.yaw = 8;
    n.shadow_angles = [1, 2, 3];
    n.shadow_y = -4.;
    let mut ops = Vec::new();
    m.display(Pass::Models, false, false, |_| -65., |op| ops.push(op));
    assert_eq!(
        ops,
        [
            BindTexture(Pass::Models),
            Push,
            Translate([5., 10., 15.]),
            RotateY(8),
            DrawModel(Mesh::RingL2),
            Pop,
            Push,
            Pop
        ]
    );
    ops.clear();
    m.display(
        Pass::Shadows,
        false,
        true,
        |_| panic!("shadow pass has no camera-depth gate"),
        |op| ops.push(op),
    );
    assert_eq!(
        ops,
        [
            FogEnabled(false),
            ApplyFog,
            BindTexture(Pass::Shadows),
            Push,
            Translate([5., -4., 15.]),
            RotateZ(3),
            RotateX(1),
            RotateY(2),
            Translate([0., 0.5, 0.]),
            Scale([0.8, 1., 0.4]),
            DrawShadow,
            Pop,
            Push,
            Pop,
            FogEnabled(true),
            ApplyFog
        ]
    );
    m.display(
        Pass::Shadows,
        true,
        true,
        |_| unreachable!(),
        |_| panic!("disabled pass changed render state"),
    );
}
