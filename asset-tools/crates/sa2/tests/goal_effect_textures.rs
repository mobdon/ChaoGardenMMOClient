use sa2_asset_tools::{goal_ring::Program, manifest::sha256};
use std::{env, fs, path::PathBuf};

#[test]
#[ignore = "requires installed stages and an independent ArchiveTool -png conversion"]
fn installed_effect_frames_match_reference() {
    let gd = PathBuf::from(env::var_os("SA2_SET_DIR").unwrap());
    let reference = PathBuf::from(env::var_os("SA2_EFFECT_REFERENCE").unwrap());
    let archive = fs::read(gd.join("efftex_common.prs")).unwrap();
    for level in [
        3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 18, 21, 22, 23, 24, 25, 26, 27, 28, 30,
        32, 40, 43, 44,
    ] {
        let p = Program::read(
            level,
            &fs::read(gd.join(format!("stg{level:02}D.rel"))).unwrap(),
        )
        .unwrap();
        let receipt = p.effect_texture_receipt(&archive).unwrap();
        let frames = receipt["frames"].as_array().unwrap();
        assert_eq!(frames.len(), 4);
        for (i, f) in frames.iter().enumerate() {
            let name = format!("kiran{:02}", i + 1);
            assert_eq!(f["name"], name);
            let bytes = fs::read(reference.join(format!("{name}.png"))).unwrap();
            let mut decoder = png::Decoder::new(&bytes[..]);
            decoder.set_transformations(png::Transformations::EXPAND);
            let mut reader = decoder.read_info().unwrap();
            let mut buf = vec![0; reader.output_buffer_size()];
            let info = reader.next_frame(&mut buf).unwrap();
            assert_eq!(info.color_type, png::ColorType::Rgba);
            assert_eq!(f["width"], info.width);
            assert_eq!(f["height"], info.height);
            let pixels = &buf[..info.buffer_size()];
            assert_eq!(f["rgbaSha256"], sha256(pixels), "{level}/{name}");
            let alpha = pixels
                .iter()
                .skip(3)
                .step_by(4)
                .copied()
                .collect::<Vec<_>>();
            assert_eq!(f["alphaSha256"], sha256(&alpha));
        }
    }
}
