//! Opt-in parity against the user's ignored SA Tools conversions. No native
//! input or expected pixel arrays are included in source control.
use sa2_asset_tools::{archive, pe, texture};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn png_rgba(path: &Path) -> (u32, u32, Vec<u8>) {
    let mut decoder = png::Decoder::new(fs::File::open(path).unwrap());
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut bytes).unwrap();
    let raw = &bytes[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => raw.to_vec(),
        png::ColorType::Rgb => raw
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => raw.iter().flat_map(|v| [*v, *v, *v, 255]).collect(),
        png::ColorType::GrayscaleAlpha => raw
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        _ => panic!("PNG did not expand to pixels"),
    };
    (info.width, info.height, rgba)
}

#[test]
#[ignore = "requires SA2_GAME_PATH and SA2_LOCAL_ROOT containing verified conversions"]
fn sa_tools_texture_pixel_parity() {
    let game = std::env::var("SA2_GAME_PATH").expect("SA2_GAME_PATH");
    let local = std::env::var("SA2_LOCAL_ROOT").expect("SA2_LOCAL_ROOT");
    let mut checked = 0;
    let mut unresolved = 0;
    let mut alpha_counts = [0u64; 256];
    let mut record_alpha = |rgba: &[u8]| {
        for pixel in rgba.chunks_exact(4) {
            alpha_counts[usize::from(pixel[3])] += 1;
        }
    };
    let mut hashes = Sha256::new();
    let mut sources = Sha256::new();
    let mut corpus = vec![
        ("al_body", "textures"),
        ("al_eye", "textures"),
        ("al_icon", "textures"),
        ("al_stg_neut_tex", "textures"),
        ("al_stg_hero_tex", "textures"),
        ("al_stg_dark_tex", "textures"),
        ("al_neut_obj_tex", "textures"),
        ("e0207texture", "textures"),
    ];
    for name in pe::CHAO_UI_ARCHIVES {
        corpus.push((name.strip_suffix(".prs").unwrap(), "chao-ui"));
    }
    for (archive_name, reference_folder) in corpus {
        let source = if archive_name == "e0207texture" {
            Path::new(&game)
                .join("resource/gd_PC/event")
                .join(format!("{archive_name}.prs"))
        } else {
            Path::new(&game)
                .join("resource/gd_PC")
                .join(format!("{archive_name}.prs"))
        };
        let source_bytes = fs::read(&source).unwrap();
        let source_hash = Sha256::digest(&source_bytes);
        sources.update(archive_name.as_bytes());
        sources.update(source_hash);
        let decoded = archive::decompress_prs(&source_bytes, 128 * 1024 * 1024).unwrap();
        let entries = archive::read_gvm(&decoded).unwrap();
        let total = entries.len();
        let before = checked;
        let unknown_before = unresolved;
        for entry in entries {
            let target = Path::new(&local)
                .join("extracted")
                .join(reference_folder)
                .join(archive_name)
                .join(format!("{}.png", entry.name));
            assert!(
                target.is_file(),
                "missing verified reference {} {}",
                archive_name,
                entry.name
            );
            let header = texture::read_gvr_header(entry.data).unwrap();
            if matches!(header.data_format, 8 | 9) && header.flags & 8 == 0 {
                unresolved += 1;
                continue;
            }
            let actual = texture::decode_gvr(entry.data, None).unwrap();
            let (width, height, expected) = png_rgba(&target);
            assert_eq!(
                (actual.width, actual.height),
                (width, height),
                "{} {}",
                archive_name,
                entry.name
            );
            assert_eq!(actual.rgba, expected, "{} {}", archive_name, entry.name);
            record_alpha(&actual.rgba);
            hashes.update(source_hash);
            hashes.update(entry.name.as_bytes());
            hashes.update(Sha256::digest(&actual.rgba));
            checked += 1;
        }
        println!("{archive_name}: {total} entries, {} exact RGBA comparisons, {} explicit external-palette unknowns, source {source_hash:x}",checked-before,unresolved-unknown_before);
        assert_eq!(
            Sha256::digest(fs::read(source).unwrap()),
            source_hash,
            "source changed"
        );
    }
    // Original child palette reconstruction compared with previously verified
    // converted paletted PNGs, separate from external-palette fallback PNGs.
    let source = fs::read(Path::new(&game).join("resource/gd_PC/al_body.prs")).unwrap();
    let data = archive::decompress_prs(&source, 128 * 1024 * 1024).unwrap();
    let entries = archive::read_gvm(&data).unwrap();
    let palette_source = Path::new(&game).join("resource/gd_PC/al_nc00.GVP");
    let palette_bytes = fs::read(&palette_source).unwrap();
    let palette_hash = Sha256::digest(&palette_bytes);
    let palette = texture::read_gvp(&palette_bytes).unwrap();
    sources.update(b"al_nc00.GVP");
    sources.update(palette_hash);
    for (bank, name) in ["al_child02", "al_child03", "al_child04"]
        .iter()
        .enumerate()
    {
        let entry = entries.iter().find(|e| e.name == *name).unwrap();
        let actual =
            texture::decode_gvr(entry.data, Some(palette.bank(bank, 16).unwrap())).unwrap();
        let (width, height, expected) = png_rgba(
            &Path::new(&local)
                .join("extracted/textures/paletted")
                .join(format!("{name}.png")),
        );
        assert_eq!((actual.width, actual.height), (width, height));
        assert_eq!(actual.rgba, expected, "palette {name}");
        record_alpha(&actual.rgba);
        hashes.update(palette_hash);
        hashes.update(name.as_bytes());
        hashes.update(Sha256::digest(&actual.rgba));
        checked += 1;
    }
    assert_eq!(
        Sha256::digest(fs::read(palette_source).unwrap()),
        palette_hash,
        "palette source changed"
    );
    assert!(checked > 200, "expected substantial verified corpus");
    let transparent = alpha_counts[0];
    let partial: u64 = alpha_counts[1..255].iter().sum();
    let opaque = alpha_counts[255];
    assert!(
        transparent > 0 && partial > 0 && opaque > 0,
        "reference corpus must exercise all three alpha cases"
    );
    println!("verified {checked} RGBA textures including {transparent} transparent, {partial} partially transparent and {opaque} opaque pixels; {} external-palette mappings remain unknown after 3 verified child banks; source corpus {:x}; pixel aggregate {:x}",unresolved-3,sources.finalize(),hashes.finalize());
}

#[test]
#[ignore = "requires SA2_GAME_PATH with installed PAK and duplicate loose DDS files"]
fn pak_payload_parity_and_dds_decode() {
    let game = std::env::var("SA2_GAME_PATH").expect("SA2_GAME_PATH");
    let root = Path::new(&game).join("resource/gd_PC/event/prs");
    let source = root.join("e0360texture.pak");
    let bytes = fs::read(&source).unwrap();
    let original_hash = Sha256::digest(&bytes);
    let entries = archive::read_pak(&bytes).unwrap();
    let mut checked = 0;
    let mut decoded = 0;
    let mut digest = Sha256::new();
    for entry in entries {
        let relative = entry.name.replace('\\', "/");
        assert!(
            !relative.starts_with('/') && !relative.split('/').any(|p| p == ".."),
            "unexpected source path"
        );
        let expected = fs::read(root.join(&relative)).unwrap();
        assert_eq!(entry.data, expected, "PAK payload {relative}");
        checked += 1;
        digest.update(Sha256::digest(entry.data));
        if relative.ends_with(".dds") {
            let actual = texture::decode_dds(entry.data).unwrap();
            assert_eq!(
                actual.rgba.len(),
                actual.width as usize * actual.height as usize * 4
            );
            decoded += 1;
        }
    }
    assert_eq!(Sha256::digest(fs::read(source).unwrap()), original_hash);
    assert_eq!(checked, 118);
    assert_eq!(decoded, 117);
    println!("PAK {checked} payloads byte-identical to independently supplied loose files; {decoded} DDS base images decode; source {original_hash:x}; payload aggregate {:x}",digest.finalize());
}

#[test]
#[ignore = "requires SA2_GAME_PATH and SA2_LOCAL_ROOT containing SA3D stage decompressions"]
fn sa3d_stage_prs_parity() {
    let game = std::env::var("SA2_GAME_PATH").expect("SA2_GAME_PATH");
    let local = std::env::var("SA2_LOCAL_ROOT").expect("SA2_LOCAL_ROOT");
    for name in [
        "ChaoStgRace",
        "ChaoStgRaceNeut",
        "ChaoStgRaceHero",
        "ChaoStgRaceDark",
        "ChaoStgKarate",
    ] {
        let source = fs::read(
            Path::new(&game)
                .join("resource/gd_PC")
                .join(format!("{name}.prs")),
        )
        .unwrap();
        let expected = fs::read(
            Path::new(&local)
                .join("activities")
                .join(format!("{name}.decompressed.bin")),
        )
        .unwrap();
        assert_eq!(
            archive::decompress_prs(&source, 128 * 1024 * 1024).unwrap(),
            expected,
            "{name}"
        );
    }
}
