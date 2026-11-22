//! Explicit opt-in tests; no original files or expected asset bytes are embedded.
use sa2_asset_tools::{model, motion, stage};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(
        std::env::var_os("SA2_REFERENCE_ROOT")
            .expect("set SA2_REFERENCE_ROOT to the original client checkout containing .local"),
    )
}
fn files(path: &Path, output: &mut Vec<PathBuf>) {
    let mut entries = std::fs::read_dir(path)
        .unwrap()
        .map(|e| e.unwrap())
        .collect::<Vec<_>>();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let ty = e.file_type().unwrap();
        if ty.is_dir() {
            files(&e.path(), output);
        } else if ty.is_file() {
            output.push(e.path());
        }
    }
}

#[test]
#[ignore = "requires ignored local SA Tools 786 native extraction"]
fn complete_verified_pc_chao_native_corpus() {
    let root = root();
    let mut paths = Vec::new();
    files(&root.join(".local/extracted/pc-chao"), &mut paths);
    let mut counts = [0usize; 3];
    for p in paths {
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !matches!(ext, "sa2mdl" | "saanim" | "sa2lvl" | "sa2blvl") {
            continue;
        }
        let bytes = std::fs::read(&p).unwrap();
        match ext {
            "sa2mdl" => {
                model::read_model(&bytes).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
                counts[0] += 1;
            }
            "saanim" => {
                motion::read_motion(&bytes).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
                counts[1] += 1;
            }
            _ => {
                stage::read_stage(&bytes).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
                counts[2] += 1;
            }
        }
    }
    assert_eq!(
        counts,
        [171, 279, 24],
        "verified SA Tools 786 extraction corpus changed"
    );
    let chao = model::read_model(
        &std::fs::read(
            root.join(".local/extracted/pc-chao/Chao/models/AL_RootObject/al_ncz.sa2mdl"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(chao.nodes.len(), 40);
    assert_eq!(
        chao.attachments.len(),
        16,
        "preserve original overlays in native reader"
    );
    for name in ["alm_tati", "alm_aruki_ncz"] {
        let p = root.join(format!(
            ".local/extracted/pc-chao/Chao/models/al_motion_table/{name}.saanim"
        ));
        let m = motion::read_motion(&std::fs::read(p).unwrap()).unwrap();
        assert_eq!(m.part_count, 40);
    }
}

#[test]
#[ignore = "requires ignored verified cinematic Blender conversion report and native models"]
fn cinematic_mesh_count_bounds_and_alpha_parity() {
    let root = root();
    let records: Value = serde_json::from_slice(
        &std::fs::read(root.join(".local/imports/cinematic-conversion-report.json")).unwrap(),
    )
    .unwrap();
    let records = records.as_array().unwrap();
    assert_eq!(records.len(), 8);
    for record in records {
        let relative = record["nativeModel"].as_str().unwrap().replace('\\', "/");
        let bytes = std::fs::read(root.join(relative)).unwrap();
        let model = model::read_model(&bytes).unwrap();
        let vertices = model
            .attachments
            .iter()
            .flat_map(|a| &a.vertex_chunks)
            .map(|c| c.vertices.len())
            .sum::<usize>();
        assert_eq!(vertices as u64, record["vertices"].as_u64().unwrap());
        assert_eq!(
            model.attachments.len() as u64,
            record["meshCount"].as_u64().unwrap()
        );
        if record["cinematic"].as_bool() != Some(true) {
            continue;
        }
        assert_eq!(model.nodes.len(), 1);
        let node = &model.nodes[0];
        assert_eq!(node.rotation_bams, [0; 3]);
        assert_eq!(node.scale, [1.; 3]);
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for vertex in model
            .attachments
            .iter()
            .flat_map(|a| &a.vertex_chunks)
            .flat_map(|c| &c.vertices)
        {
            // Explicit comparison adapter only: SAIO's native-to-Blender axes.
            let p = [
                vertex.position[0] + node.position[0],
                -(vertex.position[2] + node.position[2]),
                vertex.position[1] + node.position[1],
            ];
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
        for (key, bounds) in [("min", min), ("max", max)] {
            for (i, value) in bounds.iter().enumerate() {
                let reference = record["blenderBounds"][key][i].as_f64().unwrap();
                assert!(
                    (*value as f64 - reference).abs() < 0.0001,
                    "{} {key}[{i}]: {value} != {reference}",
                    record["name"]
                );
            }
        }
        let diffuse = model
            .attachments
            .iter()
            .flat_map(|a| &a.polygon_chunks)
            .find_map(|p| p.diffuse_rgba)
            .unwrap();
        assert_eq!(
            diffuse,
            [255, 255, 255, 204],
            "cinematic original material alpha must remain 0.8"
        );
    }
}

#[test]
#[ignore = "requires ignored original Chao Transporter mapped parts and source hash manifest"]
fn transporter_parts_keep_source_hashes_and_geometry() {
    use sha2::{Digest, Sha256};
    let root = root().join(".local/extracted/transporter");
    let report: Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let records = report["models"].as_array().unwrap();
    assert_eq!(records.len(), 4);
    for row in records {
        let bytes = std::fs::read(root.join(row["filename"].as_str().unwrap())).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            row["sha256"].as_str().unwrap()
        );
        let model = model::read_model(&bytes).unwrap();
        assert_eq!(model.nodes.len(), 1);
        assert_eq!(model.attachments.len(), 1);
        assert!(model.attachments[0]
            .vertex_chunks
            .iter()
            .any(|c| !c.vertices.is_empty()));
    }
}

/// The DLL reader against the SA Tools split of the same model: same tree, same vertices.
#[test]
#[ignore = "requires SA2_DATA_DLL (Steam Data_DLL.dll) and the local SA Tools extraction"]
fn chao_child_from_the_dll_matches_the_split_model() {
    let dll = std::fs::read(std::env::var_os("SA2_DATA_DLL").expect("set SA2_DATA_DLL")).unwrap();
    let ours = sa2_asset_tools::chao_dll::read_chao_model(&dll, 0).unwrap();
    let split = model::read_model(
        &std::fs::read(
            root().join(".local/extracted/pc-chao/Chao/models/AL_RootObject/al_ncz.sa2mdl"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(ours.nodes.len(), 40);
    assert_eq!(ours.attachments.len(), 16);
    for (a, b) in ours.nodes.iter().zip(&split.nodes) {
        assert_eq!(
            (a.flags, a.position, a.rotation_bams, a.scale),
            (b.flags, b.position, b.rotation_bams, b.scale)
        );
        assert_eq!(a.attach_offset == 0, b.attach_offset == 0);
    }
    for (a, b) in ours.attachments.iter().zip(&split.attachments) {
        let words = |m: &model::ChunkAttachment| {
            (
                m.vertex_chunks
                    .iter()
                    .map(|c| {
                        (
                            c.kind,
                            c.flags,
                            c.index_offset,
                            c.vertices
                                .iter()
                                .map(|v| v.raw_words.clone())
                                .collect::<Vec<_>>(),
                        )
                    })
                    .collect::<Vec<_>>(),
                // Strips only: the split tool rewrites one specular word of material chunks (DLL 0xFF7F, split 0x007F).
                m.polygon_chunks
                    .iter()
                    .filter(|c| c.kind >= 64)
                    .map(|c| (c.kind, c.flags, c.raw_words.clone()))
                    .collect::<Vec<_>>(),
                m.polygon_chunks
                    .iter()
                    .map(|c| (c.kind, c.flags, c.raw_words.len()))
                    .collect::<Vec<_>>(),
            )
        };
        assert_eq!(words(a), words(b));
    }
}

/// The stand motion's table row and keys from the DLL against the SA Tools split of the same motion.
#[test]
#[ignore = "requires SA2_DATA_DLL (Steam Data_DLL.dll) and the local SA Tools extraction"]
fn chao_stand_motion_from_the_dll_matches_the_split_motion() {
    let dll = std::fs::read(std::env::var_os("SA2_DATA_DLL").expect("set SA2_DATA_DLL")).unwrap();
    let ours = sa2_asset_tools::chao_dll::read_chao_motion(&dll, 0).unwrap();
    // Row 0 as the split's table lists it: LoopProperty 1, TransitionSpeed FFFFFFD8, EndFrame 13, PlaySpeed 0.12.
    assert_eq!((ours.mode, ours.next, ours.link_step), (1, -1, -40));
    assert_eq!(
        (ours.start_frame, ours.end_frame, ours.play_speed),
        (0., 13., 0.12)
    );
    let split = motion::read_motion(
        &std::fs::read(
            root().join(".local/extracted/pc-chao/Chao/models/al_motion_table/alm_tati.saanim"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        (ours.motion.frame_count, ours.motion.type_flags),
        (split.frame_count, split.type_flags)
    );
    // Decoded values, not raw words: the split tool stores the DLL's -0.0 as 0.0.
    let keys = |m: &motion::Motion| {
        m.parts
            .iter()
            .map(|p| {
                p.channels
                    .iter()
                    .map(|c| {
                        let keys = c
                            .keys
                            .iter()
                            .map(|k| (k.frame, k.floats.clone(), k.integers.clone()));
                        (c.kind.clone(), keys.collect::<Vec<_>>())
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(keys(&ours.motion), keys(&split));
}

/// The child Chao with its stand motion as a GLB, from the real game files only.
/// Writes the file when SA2_CHAO_GLB_OUT is set (kept local, never committed).
#[test]
#[ignore = "requires SA2_DATA_DLL and SA2_GD_PC (Steam resource/gd_PC)"]
fn chao_child_glb_from_the_game_files() {
    use sa2_asset_tools::{chao_dll, chao_glb, chao_palette};
    let dll = std::fs::read(std::env::var_os("SA2_DATA_DLL").expect("set SA2_DATA_DLL")).unwrap();
    let gd = PathBuf::from(std::env::var_os("SA2_GD_PC").expect("set SA2_GD_PC"));
    let model = chao_dll::read_chao_model(&dll, 0).unwrap();
    let stand = chao_dll::read_chao_motion(&dll, 0).unwrap();
    let pngs = chao_glb::body_textures(
        &model,
        &std::fs::read(gd.join("al_body.prs")).unwrap(),
        &chao_palettes(&gd),
        &chao_palette::Ratios::CHILD,
    )
    .unwrap();
    let glb = chao_glb::export(
        &model,
        &pngs,
        &[("alm_tati", &stand)],
        &chao_glb::OPEN_EYES_NO_MOUTH,
    )
    .unwrap();
    let json_len = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
    let doc: Value = serde_json::from_slice(&glb[20..20 + json_len]).unwrap();
    assert_eq!(doc["nodes"].as_array().unwrap().len(), 40);
    assert_eq!(
        doc["nodes"][0]["name"].as_str().unwrap().get(..11),
        Some("000_object_")
    );
    assert_eq!(doc["animations"][0]["name"], "alm_tati");
    for node in chao_glb::OPEN_EYES_NO_MOUTH {
        assert!(doc["nodes"][node]["mesh"].is_null());
    }
    assert!(doc["nodes"][18]["mesh"].is_number() && doc["nodes"][21]["mesh"].is_number());
    let speed =
        &doc["asset"]["extras"]["chaoBehaviour"]["clips"][0]["tableProperties"]["PlaySpeed"];
    assert_eq!(speed.as_f64().unwrap() as f32, 0.12);
    if let Some(out) = std::env::var_os("SA2_CHAO_GLB_OUT") {
        std::fs::write(out, &glb).unwrap();
    }
}

fn chao_palettes(gd: &std::path::Path) -> Vec<sa2_asset_tools::chao_palette::Colors> {
    use sa2_asset_tools::chao_palette::{read_slots, SLOT_FILES};
    let files: Vec<u8> = SLOT_FILES
        .iter()
        .flat_map(|name| std::fs::read(gd.join(format!("{name}.GVP"))).unwrap())
        .collect();
    read_slots(&files).unwrap()
}

/// Every model of AL_RootObject converts, each with the type whose palette row holds its
/// indexed textures (family = index / 6, type = family + 2, children all type 2); and a new
/// child's blend is al_nc00 itself.
#[test]
#[ignore = "requires SA2_DATA_DLL and SA2_GD_PC (Steam resource/gd_PC)"]
fn every_chao_body_converts_with_its_blended_palette() {
    use sa2_asset_tools::{chao_dll, chao_glb, chao_palette};
    let dll = std::fs::read(std::env::var_os("SA2_DATA_DLL").expect("set SA2_DATA_DLL")).unwrap();
    let gd = PathBuf::from(std::env::var_os("SA2_GD_PC").expect("set SA2_GD_PC"));
    let body = std::fs::read(gd.join("al_body.prs")).unwrap();
    let slots = chao_palettes(&gd);
    let child = chao_palette::blend(&slots, &chao_palette::Ratios::CHILD).unwrap();
    assert_eq!(child, Some(slots[0]));
    for index in 0..144 {
        let family = index / 6;
        let ratios = chao_palette::Ratios {
            chao_type: if family < 3 { 2 } else { family as u8 + 2 },
            h_pos: 0.4,
            v_pos: -0.7,
            a_pos: if family == 2 { -1. } else { 1. },
            growth: 0.5,
        };
        let model = chao_dll::read_chao_model(&dll, index).unwrap();
        chao_glb::body_textures(&model, &body, &slots, &ratios)
            .unwrap_or_else(|e| panic!("model {index}: {e}"));
    }
}
