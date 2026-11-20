use std::{env, fs, path::Path};
use tiny_shop_presentation::{Kind, read_source, render};

#[test]
#[ignore = "Set A2NE_ROM to the supplied local USA revision 0 cartridge"]
fn supplied_source_matches_every_pinned_subject_and_rejects_corruption_or_variant() {
    let source = read_source(Path::new(&env::var_os("A2NE_ROM").expect("Set A2NE_ROM"))).unwrap();
    let export = render(&source).unwrap();
    assert_eq!(export.subjects.len(), 22);
    assert_eq!(
        export.font.matched_native_labels,
        ["Mood", "Swim", "Stamina"]
    );
    assert_eq!(export.font.images.len(), 3);
    for (image, (text, width, height)) in export.font.images.iter().zip([
        ("Luck", 20, 8),
        ("Intelligence", 51, 8),
        ("ASCII 32..126", 128, 48),
    ]) {
        assert_eq!(
            (image.text, image.width, image.height),
            (text, width, height)
        );
        let mut reader = png::Decoder::new(std::io::Cursor::new(&image.png))
            .read_info()
            .unwrap();
        let mut rgba = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut rgba).unwrap();
        assert_eq!((info.width as usize, info.height as usize), (width, height));
        assert!(rgba.as_chunks::<4>().0.iter().any(|pixel| pixel[3] == 0));
        assert!(rgba.as_chunks::<4>().0.iter().all(|pixel| {
            [
                [0, 0, 0, 0],
                [0, 16, 74, 255],
                [99, 107, 115, 255],
                [198, 198, 165, 255],
            ]
            .contains(pixel)
        }));
    }
    assert_eq!(
        export
            .subjects
            .iter()
            .filter(|s| s.subject.kind == Kind::Fruit)
            .count(),
        7
    );
    for image in &export.subjects {
        let mut reader = png::Decoder::new(std::io::Cursor::new(&image.png))
            .read_info()
            .unwrap();
        let mut rgba = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut rgba).unwrap();
        assert_eq!((info.width, info.height), (16, 16));
        assert!(rgba.as_chunks::<4>().0.iter().any(|pixel| pixel[3] == 0));
        assert!(rgba.as_chunks::<4>().0.iter().any(|pixel| pixel[3] == 255));
        assert!(
            rgba.as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| pixel[3] == 0 || pixel[3] == 255)
        );
    }
    let mut corrupt = source.clone();
    corrupt[0x200] ^= 1;
    assert!(render(&corrupt).is_err());
    assert!(render(&source[..source.len() - 1]).is_err());
    let mut wrong_variant = source;
    wrong_variant[0xac..0xb0].copy_from_slice(b"ASOE");
    assert!(render(&wrong_variant).is_err());
    for name in ["ASOE_ROM", "A3VE_ROM"] {
        if let Some(path) = env::var_os(name) {
            let other = fs::read(path).unwrap();
            assert!(
                render(&other).is_err(),
                "{name} must not reuse A2NE offsets"
            );
        }
    }
}
