use sa2_asset_tools::event_camera::sample;

#[test]
fn rejects_invalid_motion_selection() {
    assert!(sample(&[0; 8], 0, 0.0).is_err());
    assert!(sample(&[0; 8], -1, 0.0).is_err());
    assert!(sample(&[0; 8], 0, f32::NAN).is_err());
}

#[test]
#[ignore = "requires SA2_EVENT_DIR and SA2_CAMERA_ORACLE from event_camera_reference.ps1"]
fn matching_sa_tools_camera_evaluations() {
    use std::{env, fs, path::PathBuf};
    let reference = fs::read(env::var_os("SA2_CAMERA_ORACLE").unwrap()).unwrap();
    let dir = PathBuf::from(env::var_os("SA2_EVENT_DIR").unwrap());
    let word = |s: &[u8], i| u32::from_le_bytes(s[i..i + 4].try_into().unwrap());
    let vector =
        |s: &[u8], i| std::array::from_fn::<_, 3, _>(|n| f32::from_bits(word(s, i + 4 * n)));
    let mut offset = 0;
    let mut tracks = 0;
    let mut count = 0;
    while offset < reference.len() {
        let header = &reference[offset..offset + 76];
        offset += 76;
        let id = word(header, 0);
        let index = word(header, 4) as i32;
        let frames = word(header, 8);
        let bank = fs::read(dir.join(format!("e{id:04}motion.bin"))).unwrap();
        let source = sample(&bank, index, 0.0).unwrap();
        assert_eq!(source.frame_count, frames);
        let a = &source.attributes;
        assert_eq!(a.position, vector(header, 12));
        assert_eq!(a.direction_z, vector(header, 24));
        assert_eq!(
            (a.roll_bams, a.fov_bams),
            (word(header, 36) as i32, word(header, 40) as i32)
        );
        assert_eq!(
            (a.near_clip, a.far_clip),
            (
                f32::from_bits(word(header, 44)),
                f32::from_bits(word(header, 48))
            )
        );
        assert_eq!(a.direction_x, vector(header, 52));
        assert_eq!(a.direction_y, vector(header, 64));
        for half in 0..frames * 2 {
            let row = &reference[offset..offset + 32];
            offset += 32;
            let frame = half as f32 * 0.5;
            let s = sample(&bank, index, frame).unwrap();
            assert_eq!(
                s.position,
                vector(row, 0),
                "event {id}, motion {index}, frame {frame}"
            );
            assert_eq!(
                s.target,
                vector(row, 12),
                "event {id}, motion {index}, frame {frame}"
            );
            assert_eq!(
                (s.roll_bams, s.fov_bams),
                (word(row, 24) as i32, word(row, 28) as i32),
                "event {id}, motion {index}, frame {frame}"
            );
            count += 1;
        }
        assert!(sample(&bank, index, frames as f32).is_err());
        tracks += 1;
    }
    assert_eq!((tracks, count), (13, 7780));
}
