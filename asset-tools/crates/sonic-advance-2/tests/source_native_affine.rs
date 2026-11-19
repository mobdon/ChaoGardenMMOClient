use serde::Deserialize;
use sonic_advance_2_assets::{
    VerifiedRom, gba,
    native_animation::{AffineTransform, AnimationPlayer, FrameDimensions, native_transform},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Oracle {
    seed: u32,
    cases: usize,
    row_words: usize,
    output_bytes: usize,
    output_sha256: String,
    routine_offset: usize,
    routine_bytes: usize,
    routine_sha256: String,
    images: Vec<Image>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Image {
    variant: u8,
    show: usize,
    frame: u16,
    rotation: u16,
    scale_x: i16,
    scale_y: i16,
    double_size: bool,
    width: usize,
    height: usize,
    origin_x: i16,
    origin_y: i16,
    rgba_sha256: String,
}

fn next(seed: &mut u32) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed
}

fn dimensions(rom: &VerifiedRom<'_>, animation: u16, frame: u16) -> FrameDimensions {
    let u32_at = |offset| {
        u32::from_le_bytes(rom.range("rom", offset, 4).unwrap().try_into().unwrap()) as usize
    };
    let table = u32_at(0xf40d8) - 0x08000000;
    let base = u32_at(table + usize::from(animation) * 4) - 0x08000000;
    let raw = rom
        .range("rom", base + usize::from(frame) * 12, 12)
        .unwrap();
    let u16_at = |offset| u16::from_le_bytes([raw[offset], raw[offset + 1]]);
    FrameDimensions {
        flip: raw[0],
        oam_index: raw[1],
        subframes: u16_at(2),
        width: u16_at(4),
        height: u16_at(6),
        origin_x: u16_at(8) as i16,
        origin_y: u16_at(10) as i16,
    }
}

#[test]
#[ignore = "A2NE_ROM and A2NE_AFFINE_ORACLE required; native_affine_oracle.py executes actual verified ROM Thumb as a test oracle"]
fn native_affine_matrices_and_anchors_match_actual_cartridge_routine() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").expect("A2NE_ROM")).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let expected: Oracle =
        serde_json::from_str(include_str!("../catalog/native-affine-parity.json")).unwrap();
    assert_eq!(
        gba::sha256(
            rom.range("rom", expected.routine_offset, expected.routine_bytes)
                .unwrap()
        ),
        expected.routine_sha256
    );
    let source =
        std::fs::read(std::env::var_os("A2NE_AFFINE_ORACLE").expect("A2NE_AFFINE_ORACLE")).unwrap();
    assert_eq!(source.len(), expected.output_bytes);
    assert_eq!(gba::sha256(&source), expected.output_sha256);
    assert_eq!(expected.row_words, 6);
    assert_eq!(source.len(), expected.cases * expected.row_words * 4);
    let scales = [
        -32768, -512, -384, -256, -192, -128, -1, 1, 128, 192, 256, 384, 512, 32767,
    ];
    let mut seed = expected.seed;
    for (row, result) in source.chunks_exact(expected.row_words * 4).enumerate() {
        let values: [u32; 6] = std::array::from_fn(|_| next(&mut seed));
        let animation = [0, 9, 10, 11][(values[0] % 4) as usize];
        let dims = dimensions(&rom, animation, (values[1] % 4) as u16);
        let transform = AffineTransform {
            rotation: (row % 1024) as u16 | (values[0] as u16 & 0xfc00),
            q_scale_x: scales[values[4] as usize % scales.len()],
            q_scale_y: scales[values[5] as usize % scales.len()],
        };
        let actual =
            native_transform(&rom, dims, transform, values[2] as i16, values[3] as i16).unwrap();
        let actual = [
            actual.pa,
            actual.pb,
            actual.pc,
            actual.pd,
            actual.sprite_x,
            actual.sprite_y,
        ]
        .map(i32::from);
        let source: Vec<_> = result
            .as_chunks::<4>()
            .0
            .iter()
            .map(|value| i32::from_le_bytes(*value))
            .collect();
        assert_eq!(
            actual.as_slice(),
            source,
            "row {row}: {transform:?}, {dims:?}"
        );
    }
}

#[test]
#[ignore = "A2NE_ROM required; metadata comes from actual cartridge transforms and independent ROM-backed OBJ sampling"]
fn native_sonic_walk_affine_pixels_match_independent_native_obj_composition() {
    let bytes = std::fs::read(std::env::var_os("A2NE_ROM").expect("A2NE_ROM")).unwrap();
    let rom = VerifiedRom::parse(&bytes).unwrap();
    let expected: Oracle =
        serde_json::from_str(include_str!("../catalog/native-affine-parity.json")).unwrap();
    assert_eq!(expected.images.len(), 360);
    let mut checked = 0;
    for variant in 0..6 {
        let mut player = AnimationPlayer::new(&rom, 9, variant).unwrap();
        for show in 0..6 {
            let mut found = false;
            for _ in 0..256 {
                if player.advance().unwrap().changed_frame {
                    found = true;
                    break;
                }
            }
            assert!(found, "walking show command budget");
            for image in expected
                .images
                .iter()
                .filter(|image| image.variant == variant && image.show == show)
            {
                let frame = player
                    .render_affine(
                        AffineTransform {
                            rotation: image.rotation,
                            q_scale_x: image.scale_x,
                            q_scale_y: image.scale_y,
                        },
                        image.double_size,
                    )
                    .unwrap();
                assert_eq!(frame.frame_index, Some(image.frame));
                assert_eq!(
                    (frame.width, frame.height, frame.origin_x, frame.origin_y),
                    (image.width, image.height, image.origin_x, image.origin_y)
                );
                assert_eq!(
                    gba::sha256(&frame.rgba),
                    image.rgba_sha256,
                    "variant {variant}, show {show}, rotation {}, scale {}",
                    image.rotation,
                    image.scale_x
                );
                assert_eq!((frame.translation_x, frame.translation_y), (0, 0));
                checked += 1;
            }
        }
    }
    assert_eq!(checked, expected.images.len());
}
