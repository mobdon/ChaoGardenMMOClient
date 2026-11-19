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
