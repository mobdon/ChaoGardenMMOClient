//! Deterministic synthetic malformed-input checks; no original fixture bytes.
use sa2_asset_tools::{archive, audio, model, motion, pe, stage, texture};

fn exercise(input: &[u8]) {
    // Conservative explicit output budgets also apply when random control bits
    // happen to form valid compressed/audio streams.
    let _ = archive::decompress_prs(input, 4096);
    let _ = archive::read_gvm(input);
    let _ = archive::read_pak(input);
    let _ = audio::read_afs(input);
    let _ = audio::read_adx_header(input);
    let _ = audio::decode_adx(input, 1024);
    let _ = model::read_model(input);
    let _ = model::read_nodes_at(input, 16, input.len());
    let _ = motion::read_motion(input);
    let _ = pe::read_pe(input);
    let _ = stage::read_stage(input);
    let _ = texture::read_gvp(input);
    let _ = texture::read_gvr_header(input);
    // Dimension bytes are clamped for decoder fuzzing below. This generic
    // exercise only reads texture headers to avoid large repeated allocations.
}

fn synthetic_model() -> Vec<u8> {
    let mut b = vec![0; 68];
    b[..8].copy_from_slice(b"SA2MDL\0\x03");
    b[8..12].copy_from_slice(&16u32.to_le_bytes());
    for i in [48, 52, 56] {
        b[i..i + 4].copy_from_slice(&1f32.to_le_bytes());
    }
    b
}

fn synthetic_motion() -> Vec<u8> {
    let mut b = vec![0; 56];
    b[..8].copy_from_slice(b"SAANIM\0\x02");
    b[8..12].copy_from_slice(&20u32.to_le_bytes());
    b[16..20].copy_from_slice(&1u32.to_le_bytes());
    b[20..24].copy_from_slice(&32u32.to_le_bytes());
    b[24..28].copy_from_slice(&10u32.to_le_bytes());
    b[28..30].copy_from_slice(&1u16.to_le_bytes());
    b[30..32].copy_from_slice(&1u16.to_le_bytes());
    b[32..36].copy_from_slice(&40u32.to_le_bytes());
    b[36..40].copy_from_slice(&1u32.to_le_bytes());
    b
}

fn synthetic_stage() -> Vec<u8> {
    let mut b = vec![0; 48];
    b[..8].copy_from_slice(b"SA2BLVL\x03");
    b[8..12].copy_from_slice(&16u32.to_le_bytes());
    b
}

fn synthetic_pe() -> Vec<u8> {
    let mut b = vec![0; 1024];
    b[..2].copy_from_slice(b"MZ");
    b[60..64].copy_from_slice(&128u32.to_le_bytes());
    b[128..132].copy_from_slice(b"PE\0\0");
    for (at, n) in [(132, 0x14cu16), (134, 1), (148, 224), (152, 0x10b)] {
        b[at..at + 2].copy_from_slice(&n.to_le_bytes());
    }
    for (at, n) in [
        (180, 0x400000u32),
        (212, 512),
        (244, 16),
        (384, 512),
        (388, 0x1000),
        (392, 512),
        (396, 512),
    ] {
        b[at..at + 4].copy_from_slice(&n.to_le_bytes());
    }
    b
}

fn synthetic_afs() -> Vec<u8> {
    let mut b = b"AFS\0".to_vec();
    b.extend(1u32.to_le_bytes());
    b.extend(16u32.to_le_bytes());
    b.extend(0u32.to_le_bytes());
    b
}

fn synthetic_adx() -> Vec<u8> {
    let mut b = vec![0; 32];
    b[..4].copy_from_slice(&[0x80, 0, 0, 28]);
    b[4..8].copy_from_slice(&[3, 18, 4, 1]);
    b[8..12].copy_from_slice(&44100u32.to_be_bytes());
    b[16..18].copy_from_slice(&500u16.to_be_bytes());
    b[18..20].copy_from_slice(&0x0300u16.to_be_bytes());
    b[26..32].copy_from_slice(b"(c)CRI");
    b
}

#[test]
fn every_truncation_and_hostile_pointer_is_panic_free() {
    assert!(model::read_model(&synthetic_model()).is_ok());
    assert!(motion::read_motion(&synthetic_motion()).is_ok());
    assert!(stage::read_stage(&synthetic_stage()).is_ok());
    assert!(pe::read_pe(&synthetic_pe()).is_ok());
    assert!(audio::read_afs(&synthetic_afs()).is_ok());
    assert!(audio::decode_adx(&synthetic_adx(), 1024).is_ok());
    let mut fixtures = vec![
        synthetic_model(),
        synthetic_motion(),
        synthetic_stage(),
        synthetic_pe(),
        synthetic_afs(),
        synthetic_adx(),
    ];
    let mut pak = vec![0; 0x3d];
    pak[..4].copy_from_slice(b"\x01pak");
    fixtures.push(pak);
    let mut gvm = b"GVMH".to_vec();
    gvm.extend(4u32.to_le_bytes());
    gvm.extend([0; 4]);
    fixtures.push(gvm);
    for fixture in &fixtures {
        for n in 0..=fixture.len() {
            assert!(
                std::panic::catch_unwind(|| exercise(&fixture[..n])).is_ok(),
                "panic at truncation {n}"
            );
        }
        for at in (0..fixture.len().saturating_sub(3)).step_by(4) {
            for word in [u32::MAX, 0x8000_0000, 0x7fff_ffff, 1, 0] {
                let mut mutated = fixture.clone();
                mutated[at..at + 4].copy_from_slice(&word.to_le_bytes());
                assert!(
                    std::panic::catch_unwind(|| exercise(&mutated)).is_ok(),
                    "panic at field {at:#x}={word:#x}"
                );
            }
        }
    }
}

#[test]
fn deterministic_random_bytes_and_header_mutations_are_panic_free() {
    let mut seed = 0x5a2b_7860_2026_1118u64;
    for n in 0..512 {
        let mut bytes = vec![0; n];
        for b in &mut bytes {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            *b = seed as u8;
        }
        assert!(std::panic::catch_unwind(|| exercise(&bytes)).is_ok());
        if n >= 20 {
            for magic in [b"SA2MDL\0\x03", b"SAANIM\0\x02", b"SA2BLVL\x03"] {
                bytes[..8].copy_from_slice(magic);
                assert!(std::panic::catch_unwind(|| exercise(&bytes)).is_ok());
            }
        }
    }
}

#[test]
fn small_rectangular_texture_shapes_and_truncations_are_panic_free() {
    let palette = [[1, 2, 3, 4]; 256];
    for format in [0, 1, 2, 3, 4, 5, 6, 8, 9, 14, 255] {
        for w in [1u16, 2, 4, 8, 16] {
            for h in [1u16, 2, 4, 8, 16] {
                let mut b = b"GVRT".to_vec();
                b.extend(520u32.to_le_bytes());
                b.extend([0, 0, 0, format]);
                b.extend(w.to_be_bytes());
                b.extend(h.to_be_bytes());
                b.extend([0x9a; 512]);
                for end in [16, 24, 48, 80, 144, b.len()] {
                    let mut truncated = b[..end].to_vec();
                    truncated[4..8].copy_from_slice(&((end - 8) as u32).to_le_bytes());
                    assert!(
                        std::panic::catch_unwind(|| texture::decode_gvr(
                            &truncated,
                            Some(&palette)
                        ))
                        .is_ok(),
                        "GVR panic: f{format} {w}x{h} payload{}",
                        end - 16
                    );
                }
            }
        }
    }
    for code in [b"DXT1", b"DXT3", b"DXT5", b"DX10"] {
        let mut dds = [0; 160];
        dds[..4].copy_from_slice(b"DDS ");
        dds[4..8].copy_from_slice(&124u32.to_le_bytes());
        dds[76..80].copy_from_slice(&32u32.to_le_bytes());
        dds[80..84].copy_from_slice(&4u32.to_le_bytes());
        dds[84..88].copy_from_slice(code);
        for w in [1u32, 2, 4, 8] {
            for h in [1u32, 2, 4, 8] {
                dds[16..20].copy_from_slice(&w.to_le_bytes());
                dds[12..16].copy_from_slice(&h.to_le_bytes());
                for n in 0..=dds.len() {
                    assert!(std::panic::catch_unwind(|| texture::decode_dds(&dds[..n])).is_ok());
                }
            }
        }
    }
}
