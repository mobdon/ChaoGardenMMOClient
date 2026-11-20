use sa2_asset_tools::finalhazard_visual::BossMeshClip;

#[test]
#[ignore = "requires SA2_GAME_PATH pointing to the installed game directory"]
fn all_authored_frames_keep_boss_topology() {
    let exe = std::fs::read(
        std::path::PathBuf::from(std::env::var_os("SA2_GAME_PATH").unwrap()).join("sonic2app.exe"),
    )
    .unwrap();
    for (index, frames) in [140, 64, 65, 160].into_iter().enumerate() {
        let clip = BossMeshClip::load(&exe, index).unwrap();
        assert_eq!(clip.frame_count(), frames);
        for f in 0..frames {
            let mesh = clip.sample(f64::from(f)).unwrap();
            assert_eq!(mesh.vertices.len(), 1702);
            assert_eq!(mesh.surfaces.len(), 64);
            assert_eq!(
                mesh.surfaces
                    .iter()
                    .flat_map(|s| &s.strips)
                    .map(|s| s.triangles.len())
                    .sum::<usize>(),
                2996
            );
            assert!(mesh
                .vertices
                .iter()
                .all(|v| v.world_position.iter().all(|x| x.is_finite())));
        }
        assert!(clip.sample(f64::from(frames)).is_err());
    }
}
