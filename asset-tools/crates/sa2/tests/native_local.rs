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
